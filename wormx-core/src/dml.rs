//! Typed `INSERT`, `UPDATE`, `DELETE`, and `UPSERT` builders.
//!
//! Each builder renders its command through the dialect's operation renderer
//! and executes it on a SQLx pool or transaction.

use std::marker::PhantomData;

use sqlx_core::database::Database;
use sqlx_core::error::Result;
use sqlx_core::executor::Executor;
use sqlx_core::from_row::FromRow;
use sqlx_core::query::query_with;
use sqlx_core::query_as::query_as_with;
use sqlx_core::sql_str::{AssertSqlSafe, SqlSafeStr};

use crate::db::{Db, Transaction};
use crate::dialect::{Dialect, Expr, IntoExpr, SqlWriter};
use crate::join::TableSource;
use crate::metadata::Table;
use crate::query::{
    Column, DeletePlan, DeleteRenderer, InsertPlan, InsertRenderer, UpdatePlan, UpdateRenderer,
    UpsertPlan, UpsertRenderer,
};

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

    /// Executes `INSERT ... RETURNING *` and decodes the inserted row.
    pub async fn exec_returning(mut self) -> Result<T>
    where
        T: Send + Unpin + for<'r> FromRow<'r, <D::DB as Database>::Row>,
        <D::DB as Database>::Arguments: sqlx_core::arguments::IntoArguments<D::DB>,
        for<'c> &'c mut <D::DB as Database>::Connection: Executor<'c, Database = D::DB>,
    {
        let (sql, arguments) = self.render(true)?;
        query_as_with::<D::DB, T, _>(AssertSqlSafe(sql).into_sql_str(), arguments)
            .fetch_one(self.db.pool())
            .await
    }

    /// Executes `INSERT ... RETURNING *` inside a transaction.
    pub async fn exec_returning_in(mut self, tx: &mut Transaction<'_, D>) -> Result<T>
    where
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
        let mut writer = SqlWriter::new(self.db.dialect());
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
        let mut writer = SqlWriter::new(self.db.dialect());
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

/// A pending `DELETE`.
pub struct Delete<D: Dialect + Clone, T: Table> {
    db: Db<D>,
    source: TableSource,
    filters: Vec<Expr<bool, D>>,
    _marker: PhantomData<fn() -> T>,
}

impl<D: Dialect + Clone, T: Table> Delete<D, T> {
    pub(crate) fn new(db: Db<D>) -> Self {
        Self {
            db,
            source: TableSource::new(T::DEF, None),
            filters: Vec::new(),
            _marker: PhantomData,
        }
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
        let mut writer = SqlWriter::new(self.db.dialect());
        D::Delete::default().write(
            DeletePlan {
                table: self.source,
                filters: &mut self.filters,
                returning,
            },
            &mut writer,
        )?;
        Ok(writer.finish())
    }
}

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
        T: Send + Unpin + for<'r> FromRow<'r, <D::DB as Database>::Row>,
        <D::DB as Database>::Arguments: sqlx_core::arguments::IntoArguments<D::DB>,
        for<'c> &'c mut <D::DB as Database>::Connection: Executor<'c, Database = D::DB>,
    {
        let (sql, arguments) = self.render(true)?;
        query_as_with::<D::DB, T, _>(AssertSqlSafe(sql).into_sql_str(), arguments)
            .fetch_one(self.db.pool())
            .await
    }

    fn render(&mut self, returning: bool) -> Result<(String, <D::DB as Database>::Arguments)> {
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
