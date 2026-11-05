//! The `UPSERT` builder.

use std::marker::PhantomData;

use sqlx_core::database::Database;
use sqlx_core::error::{Error, Result};
use sqlx_core::executor::Executor;
use sqlx_core::from_row::FromRow;
use sqlx_core::query::query_with;
use sqlx_core::query_as::query_as_with;
use sqlx_core::sql_str::{AssertSqlSafe, SqlSafeStr};

use crate::exec::{Db, Transaction};
use crate::plan::join::TableSource;
use crate::plan::query::{Column, UpsertPlan, UpsertRenderer};
use crate::schema::Table;
use crate::sql::{Dialect, Expr, IntoExpr, SqlWriter, SupportsReturning};

/// A pending upsert, rendered by the dialect (`ON CONFLICT`, `ON DUPLICATE
/// KEY UPDATE`, or `MERGE`).
pub struct Upsert<D: Dialect + Clone, T: Table> {
    db: Db<D>,
    source: TableSource,
    columns: Vec<&'static str>,
    values: Vec<Expr<(), D>>,
    conflict_columns: Vec<&'static str>,
    assignments: Vec<(&'static str, Expr<(), D>)>,
    _marker: PhantomData<fn() -> T>,
}

impl<D: Dialect + Clone, T: Table> Upsert<D, T> {
    pub(crate) fn new(db: Db<D>) -> Self {
        Self {
            db,
            source: TableSource::new(T::DEF, None),
            columns: Vec::new(),
            values: Vec::new(),
            conflict_columns: Vec::new(),
            assignments: Vec::new(),
            _marker: PhantomData,
        }
    }

    /// Sets a column value.
    pub fn value<C, V>(mut self, column: Column<C>, value: V) -> Self
    where
        C: 'static,
        V: IntoExpr<C, D>,
    {
        self.columns.push(column.name());
        self.values.push(value.into_expr().erase());
        self
    }

    /// The conflict target columns, for example the primary key.
    pub fn conflict_on<C>(mut self, column: Column<C>) -> Self {
        self.conflict_columns.push(column.name());
        self
    }

    /// Attaches an opaque, dialect-defined suffix to the target table.
    pub fn with_source_suffix(mut self, suffix: &'static str) -> Self {
        self.source.suffix = Some(suffix);
        self
    }

    /// A column to update when the row already exists.
    pub fn update<C, V>(mut self, column: Column<C>, value: V) -> Self
    where
        C: 'static,
        V: IntoExpr<C, D>,
    {
        self.assignments
            .push((column.name(), value.into_expr().erase()));
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

    /// Executes the statement and returns the affected row.
    pub async fn exec_returning(mut self) -> Result<T>
    where
        D: SupportsReturning,
        T: Send + Unpin + for<'r> FromRow<'r, <D::DB as Database>::Row>,
        <D::DB as Database>::Arguments: sqlx_core::arguments::IntoArguments<D::DB>,
        for<'c> &'c mut <D::DB as Database>::Connection: Executor<'c, Database = D::DB>,
    {
        let (sql, arguments) = self.render(true)?;
        query_as_with::<D::DB, T, _>(AssertSqlSafe(sql).into_sql_str(), arguments)
            .fetch_one(self.db.pool())
            .await
    }

    /// Executes the statement inside a transaction and returns the affected row.
    pub async fn exec_returning_in(mut self, tx: &mut Transaction<'_, D>) -> Result<T>
    where
        D: SupportsReturning,
        T: Send + Unpin + for<'r> FromRow<'r, <D::DB as Database>::Row>,
        <D::DB as Database>::Arguments: sqlx_core::arguments::IntoArguments<D::DB>,
        for<'c> &'c mut <D::DB as Database>::Connection: Executor<'c, Database = D::DB>,
    {
        let (sql, arguments) = self.render(true)?;
        query_as_with::<D::DB, T, _>(AssertSqlSafe(sql).into_sql_str(), arguments)
            .fetch_one(tx.connection())
            .await
    }

    fn render(&mut self, returning: bool) -> Result<(String, <D::DB as Database>::Arguments)> {
        if self.columns.is_empty() {
            return Err(Error::InvalidArgument(
                "UPSERT requires at least one .value(...) for the inserted row".to_owned(),
            ));
        }

        let mut writer = SqlWriter::new(self.db.dialect());
        D::Upsert::default().write(
            UpsertPlan {
                table: self.source,
                columns: &self.columns,
                values: &mut self.values,
                conflict_columns: &self.conflict_columns,
                assignments: &mut self.assignments,
                returning,
            },
            &mut writer,
        )?;
        Ok(writer.finish())
    }
}
