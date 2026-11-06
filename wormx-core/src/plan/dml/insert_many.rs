//! The multi-row `INSERT` builder.

use std::marker::PhantomData;

use sqlx_core::database::Database;
use sqlx_core::error::Result;
use sqlx_core::executor::Executor;
use sqlx_core::from_row::FromRow;
use sqlx_core::query::query_with;
use sqlx_core::query_as::query_as_with;
use sqlx_core::sql_str::{AssertSqlSafe, SqlSafeStr};

use crate::exec::{Db, Transaction};
use crate::plan::backend::{Dialect, SupportsReturning};
use crate::plan::dml::batch::{RowBuilder, normalize_rows};
use crate::plan::query::{InsertManyPlan, InsertRenderer};
use crate::schema::Table;

/// A pending multi-row `INSERT`, rendered as one statement.
pub struct InsertMany<D: Dialect + Clone, T: Table> {
    db: Db<D>,
    rows: Vec<RowBuilder<D>>,
    _marker: PhantomData<fn() -> T>,
}

impl<D: Dialect + Clone, T: Table> InsertMany<D, T> {
    pub(crate) fn new(db: Db<D>) -> Self {
        Self {
            db,
            rows: Vec::new(),
            _marker: PhantomData,
        }
    }

    /// Adds one row.
    pub fn row<F>(mut self, build: F) -> Self
    where
        F: FnOnce(RowBuilder<D>) -> RowBuilder<D>,
    {
        self.rows.push(build(RowBuilder::new()));
        self
    }

    /// Adds one row per item produced by `items`.
    pub fn rows<I, F>(mut self, items: I, mut build: F) -> Self
    where
        I: IntoIterator,
        F: FnMut(I::Item, RowBuilder<D>) -> RowBuilder<D>,
    {
        for item in items {
            self = self.row(|row| build(item, row));
        }
        self
    }

    /// Executes the statement.
    pub async fn exec(mut self) -> Result<<D::DB as Database>::QueryResult>
    where
        <D::DB as Database>::Arguments: sqlx_core::arguments::IntoArguments<D::DB>,
        for<'c> &'c mut <D::DB as Database>::Connection: Executor<'c, Database = D::DB>,
    {
        let (sql, arguments) = self.render(false)?;
        query_with::<D::DB, _>(AssertSqlSafe(sql).into_sql_str(), arguments)
            .execute(self.db.pool())
            .await
    }

    /// Executes the statement inside a transaction.
    pub async fn exec_in(
        mut self,
        tx: &mut Transaction<'_, D>,
    ) -> Result<<D::DB as Database>::QueryResult>
    where
        <D::DB as Database>::Arguments: sqlx_core::arguments::IntoArguments<D::DB>,
        for<'c> &'c mut <D::DB as Database>::Connection: Executor<'c, Database = D::DB>,
    {
        let (sql, arguments) = self.render(false)?;
        query_with::<D::DB, _>(AssertSqlSafe(sql).into_sql_str(), arguments)
            .execute(tx.connection())
            .await
    }

    /// Executes `INSERT ... RETURNING *` and decodes every inserted row.
    pub async fn exec_returning(mut self) -> Result<Vec<T>>
    where
        D: SupportsReturning,
        T: Send + Unpin + for<'r> FromRow<'r, <D::DB as Database>::Row>,
        <D::DB as Database>::Arguments: sqlx_core::arguments::IntoArguments<D::DB>,
        for<'c> &'c mut <D::DB as Database>::Connection: Executor<'c, Database = D::DB>,
    {
        let (sql, arguments) = self.render(true)?;
        query_as_with::<D::DB, T, _>(AssertSqlSafe(sql).into_sql_str(), arguments)
            .fetch_all(self.db.pool())
            .await
    }

    /// Executes `INSERT ... RETURNING *` inside a transaction.
    pub async fn exec_returning_in(mut self, tx: &mut Transaction<'_, D>) -> Result<Vec<T>>
    where
        D: SupportsReturning,
        T: Send + Unpin + for<'r> FromRow<'r, <D::DB as Database>::Row>,
        <D::DB as Database>::Arguments: sqlx_core::arguments::IntoArguments<D::DB>,
        for<'c> &'c mut <D::DB as Database>::Connection: Executor<'c, Database = D::DB>,
    {
        let (sql, arguments) = self.render(true)?;
        query_as_with::<D::DB, T, _>(AssertSqlSafe(sql).into_sql_str(), arguments)
            .fetch_all(tx.connection())
            .await
    }

    fn render(&mut self, returning: bool) -> Result<(String, <D::DB as Database>::Arguments)> {
        let rows = std::mem::take(&mut self.rows);
        let rows = rows
            .into_iter()
            .map(RowBuilder::into_values)
            .collect::<Vec<_>>();
        let (columns, mut rows) = normalize_rows(rows)?;

        let mut writer = self.db.writer();
        D::Insert::default().write_many(
            InsertManyPlan {
                table: T::DEF,
                columns: &columns,
                rows: &mut rows,
                returning,
            },
            &mut writer,
        )?;
        Ok(writer.finish())
    }
}
