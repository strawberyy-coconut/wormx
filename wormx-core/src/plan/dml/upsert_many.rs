//! The multi-row `UPSERT` builder.

use std::marker::PhantomData;

use sqlx_core::database::Database;
use sqlx_core::error::{Error, Result};
use sqlx_core::executor::Executor;
use sqlx_core::from_row::FromRow;
use sqlx_core::query::query_with;
use sqlx_core::query_as::query_as_with;
use sqlx_core::sql_str::{AssertSqlSafe, SqlSafeStr};

use crate::exec::{Db, Transaction};
use crate::plan::backend::{Dialect, SupportsReturning};
use crate::plan::dml::batch::{RowBuilder, normalize_rows};
use crate::plan::expr::{Expr, IntoExpr};
use crate::plan::join::TableSource;
use crate::plan::query::{Column, UpsertManyPlan, UpsertRenderer};
use crate::schema::Table;

/// A pending multi-row upsert, rendered by the dialect as one statement.
pub struct UpsertMany<D: Dialect + Clone, T: Table> {
    db: Db<D>,
    source: TableSource,
    rows: Vec<RowBuilder<D>>,
    conflict_columns: Vec<&'static str>,
    assignments: Vec<(&'static str, Expr<(), D>)>,
    _marker: PhantomData<fn() -> T>,
}

impl<D: Dialect + Clone, T: Table> UpsertMany<D, T> {
    pub(crate) fn new(db: Db<D>) -> Self {
        Self {
            db,
            source: TableSource::new(T::DEF, None),
            rows: Vec::new(),
            conflict_columns: Vec::new(),
            assignments: Vec::new(),
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

    /// Adds an already-built row.
    pub fn push_row(mut self, row: RowBuilder<D>) -> Self {
        self.rows.push(row);
        self
    }

    /// Adds a conflict-target column.
    pub fn conflict_on<C>(mut self, column: Column<C>) -> Self {
        self.conflict_columns.push(column.name());
        self
    }

    /// Sets a column to a value shared by every row when the row exists.
    pub fn update<C, V>(mut self, column: Column<C>, value: V) -> Self
    where
        C: 'static,
        V: IntoExpr<C, D>,
    {
        self.assignments
            .push((column.name(), value.into_expr().erase()));
        self
    }

    /// Sets a column to the incoming row's value when the row already exists.
    ///
    /// This is the portable spelling of `excluded.column`, `VALUES(column)`,
    /// and `source.column`.
    pub fn update_from_input<C>(mut self, column: Column<C>) -> Self
    where
        C: 'static,
    {
        self.assignments
            .push((column.name(), Expr::input(column).erase()));
        self
    }

    /// Attaches an opaque, dialect-defined suffix to the target table.
    pub fn with_source_suffix(mut self, suffix: &'static str) -> Self {
        self.source.suffix = Some(suffix);
        self
    }

    /// Executes the statement.
    pub async fn exec(self) -> Result<<D::DB as Database>::QueryResult>
    where
        <D::DB as Database>::Arguments: sqlx_core::arguments::IntoArguments<D::DB>,
        for<'c> &'c mut <D::DB as Database>::Connection: Executor<'c, Database = D::DB>,
    {
        let pool = self.db.pool().clone();
        let (sql, arguments) = self.render(false)?;
        query_with::<D::DB, _>(AssertSqlSafe(sql).into_sql_str(), arguments)
            .execute(&pool)
            .await
    }

    /// Executes the statement inside a transaction.
    pub async fn exec_in(
        self,
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

    /// Executes the statement and decodes every affected row.
    pub async fn exec_returning(self) -> Result<Vec<T>>
    where
        D: SupportsReturning,
        T: Send + Unpin + for<'r> FromRow<'r, <D::DB as Database>::Row>,
        <D::DB as Database>::Arguments: sqlx_core::arguments::IntoArguments<D::DB>,
        for<'c> &'c mut <D::DB as Database>::Connection: Executor<'c, Database = D::DB>,
    {
        let pool = self.db.pool().clone();
        let (sql, arguments) = self.render(true)?;
        query_as_with::<D::DB, T, _>(AssertSqlSafe(sql).into_sql_str(), arguments)
            .fetch_all(&pool)
            .await
    }

    /// Executes the statement inside a transaction and decodes every affected
    /// row.
    pub async fn exec_returning_in(self, tx: &mut Transaction<'_, D>) -> Result<Vec<T>>
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

    fn render(mut self, returning: bool) -> Result<(String, <D::DB as Database>::Arguments)> {
        if self.conflict_columns.is_empty() {
            return Err(Error::InvalidArgument(
                "UPSERT requires at least one conflict column; call .conflict_on(...)".to_owned(),
            ));
        }
        if self.assignments.is_empty() {
            return Err(Error::InvalidArgument(
                "UPSERT requires at least one .update(...) or .update_from_input(...) assignment"
                    .to_owned(),
            ));
        }

        let rows = std::mem::take(&mut self.rows);
        let rows = rows
            .into_iter()
            .map(RowBuilder::into_values)
            .collect::<Vec<_>>();
        let (columns, mut rows) = normalize_rows(rows)?;
        if columns.is_empty() {
            return Err(Error::InvalidArgument(
                "UPSERT requires at least one .value(...) for each inserted row".to_owned(),
            ));
        }

        let mut writer = self.db.writer();
        D::Upsert::default().write_many(
            UpsertManyPlan {
                table: self.source,
                columns: &columns,
                rows: &mut rows,
                conflict_columns: &self.conflict_columns,
                assignments: &mut self.assignments,
                returning,
            },
            &mut writer,
        )?;
        Ok(writer.finish())
    }
}
