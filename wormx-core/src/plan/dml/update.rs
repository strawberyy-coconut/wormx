//! The `UPDATE` builder.

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
use crate::plan::expr::{Expr, IntoExpr};
use crate::plan::join::TableSource;
use crate::plan::query::{Column, UpdatePlan, UpdateRenderer};
use crate::schema::Table;

/// A pending `UPDATE`.
pub struct Update<D: Dialect + Clone, T: Table> {
    db: Db<D>,
    source: TableSource,
    assignments: Vec<(&'static str, Expr<(), D>)>,
    filters: Vec<Expr<bool, D>>,
    _marker: PhantomData<fn() -> T>,
}

impl<D: Dialect + Clone, T: Table> Update<D, T> {
    pub(crate) fn new(db: Db<D>) -> Self {
        Self {
            db,
            source: TableSource::new(T::DEF, None),
            assignments: Vec::new(),
            filters: Vec::new(),
            _marker: PhantomData,
        }
    }

    /// Sets a column to a value or expression.
    pub fn set<C, V>(mut self, column: Column<C>, value: V) -> Self
    where
        C: 'static,
        V: IntoExpr<C, D>,
    {
        self.assignments
            .push((column.name(), value.into_expr().erase()));
        self
    }

    /// Sets a column to a raw SQL expression.
    pub fn set_raw<C>(mut self, column: Column<C>, sql: impl Into<String>) -> Self {
        self.assignments.push((column.name(), Expr::raw(sql)));
        self
    }

    /// Attaches an opaque, dialect-defined suffix to the target table.
    pub fn with_source_suffix(mut self, suffix: &'static str) -> Self {
        self.source.suffix = Some(suffix);
        self
    }

    /// Adds a filter predicate.
    pub fn filter(mut self, filter: Expr<bool, D>) -> Self {
        self.filters.push(filter);
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

    /// Executes the statement and returns the affected rows.
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

    /// Executes the statement inside a transaction and returns the affected rows.
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
        if self.assignments.is_empty() {
            return Err(Error::InvalidArgument(
                "UPDATE requires at least one assignment; call .set(...) or .set_raw(...)"
                    .to_owned(),
            ));
        }

        let mut writer = self.db.writer();
        D::Update::default().write(
            UpdatePlan {
                table: self.source,
                assignments: &mut self.assignments,
                filters: &mut self.filters,
                returning,
            },
            &mut writer,
        )?;
        Ok(writer.finish())
    }
}
