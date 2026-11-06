//! The `INSERT` builder.

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
use crate::plan::dml::Returning;
use crate::plan::expr::{Expr, IntoExpr};
use crate::plan::query::{Column, InsertPlan, InsertRenderer};
use crate::schema::Table;

/// A pending `INSERT`.
pub struct Insert<D: Dialect + Clone, T: Table> {
    db: Db<D>,
    columns: Vec<&'static str>,
    values: Vec<Expr<(), D>>,
    _marker: PhantomData<fn() -> T>,
}

impl<D: Dialect + Clone, T: Table> Insert<D, T> {
    pub(crate) fn new(db: Db<D>) -> Self {
        Self {
            db,
            columns: Vec::new(),
            values: Vec::new(),
            _marker: PhantomData,
        }
    }

    /// Sets one column. Calling it again for the same column is an error at execution time.
    pub fn value<C, V>(mut self, column: Column<C>, value: V) -> Self
    where
        C: 'static,
        V: IntoExpr<C, D>,
    {
        self.columns.push(column.name());
        self.values.push(value.into_expr().erase());
        self
    }

    /// Sets a column to an arbitrary expression (for example `DEFAULT` or `NOW()`).
    pub fn value_expr<C>(mut self, column: Column<C>, value: Expr<(), D>) -> Self {
        self.columns.push(column.name());
        self.values.push(value);
        self
    }

    /// Appends a literal SQL expression as a value.
    pub fn value_raw<C>(mut self, column: Column<C>, sql: impl Into<String>) -> Self {
        self.values.push(Expr::raw(sql));
        self.columns.push(column.name());
        self
    }

    /// Renders `INSERT ... RETURNING *` and decodes the inserted row when
    /// executed.
    ///
    /// Available only for dialects that implement [`SupportsReturning`].
    pub fn returning(self) -> Returning<Self>
    where
        D: SupportsReturning,
    {
        Returning::new(self)
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

    fn render(&mut self, returning: bool) -> Result<(String, <D::DB as Database>::Arguments)> {
        let mut writer = self.db.writer();
        D::Insert::default().write(
            InsertPlan {
                table: T::DEF,
                columns: &self.columns,
                values: &mut self.values,
                returning,
            },
            &mut writer,
        )?;
        Ok(writer.finish())
    }
}

impl<D: Dialect + Clone, T: Table> Returning<Insert<D, T>> {
    /// Executes `INSERT ... RETURNING *` and decodes the inserted row.
    pub async fn exec(mut self) -> Result<T>
    where
        D: SupportsReturning,
        T: Send + Unpin + for<'r> FromRow<'r, <D::DB as Database>::Row>,
        <D::DB as Database>::Arguments: sqlx_core::arguments::IntoArguments<D::DB>,
        for<'c> &'c mut <D::DB as Database>::Connection: Executor<'c, Database = D::DB>,
    {
        let (sql, arguments) = self.inner.render(true)?;
        query_as_with::<D::DB, T, _>(AssertSqlSafe(sql).into_sql_str(), arguments)
            .fetch_one(self.inner.db.pool())
            .await
    }

    /// Executes `INSERT ... RETURNING *` inside a transaction.
    pub async fn exec_in(mut self, tx: &mut Transaction<'_, D>) -> Result<T>
    where
        D: SupportsReturning,
        T: Send + Unpin + for<'r> FromRow<'r, <D::DB as Database>::Row>,
        <D::DB as Database>::Arguments: sqlx_core::arguments::IntoArguments<D::DB>,
        for<'c> &'c mut <D::DB as Database>::Connection: Executor<'c, Database = D::DB>,
    {
        let (sql, arguments) = self.inner.render(true)?;
        query_as_with::<D::DB, T, _>(AssertSqlSafe(sql).into_sql_str(), arguments)
            .fetch_one(tx.connection())
            .await
    }
}
