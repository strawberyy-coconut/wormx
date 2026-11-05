//! Database handles, raw SQL queries, and transactions.

use sqlx_core::arguments::Arguments;
use sqlx_core::database::Database;
use sqlx_core::encode::Encode;
use sqlx_core::error::{BoxDynError, Error, Result};
use sqlx_core::executor::Executor;
use sqlx_core::from_row::FromRow;
use sqlx_core::pool::Pool;
use sqlx_core::query::query_with;
use sqlx_core::query_as::query_as_with;
use sqlx_core::sql_str::{AssertSqlSafe, SqlSafeStr};
use sqlx_core::transaction::Transaction as SqlxTransaction;
use sqlx_core::types::Type;

use crate::dialect::{Dialect, SupportsUpsert};
use crate::dml::{Delete, Insert, Update, Upsert};
use crate::query::{IntoSelect, Select};

/// A database handle that pairs a SQLx pool with a wormx dialect.
///
/// Cloning a `Db` shares the pool and clones the dialect.
pub struct Db<D: Dialect + Clone> {
    pool: Pool<D::DB>,
    dialect: D,
}

impl<D: Dialect + Clone> Clone for Db<D> {
    fn clone(&self) -> Self {
        Self {
            pool: self.pool.clone(),
            dialect: self.dialect.clone(),
        }
    }
}

impl<D: Dialect + Clone> Db<D> {
    /// Creates a database handle from a SQLx pool and dialect.
    pub fn new(pool: Pool<D::DB>, dialect: D) -> Self {
        Self { pool, dialect }
    }

    /// Returns the underlying SQLx pool.
    pub fn pool(&self) -> &Pool<D::DB> {
        &self.pool
    }

    /// Returns the dialect.
    pub fn dialect(&self) -> &D {
        &self.dialect
    }

    /// Starts a typed query using this database's dialect.
    pub fn query<Q>(&self, source: Q) -> Select<D, Q::Table>
    where
        Q: IntoSelect<D>,
    {
        source.into_select(self)
    }

    /// Starts an `INSERT` for a generated table.
    pub fn insert<T: crate::metadata::Table>(&self) -> Insert<D, T> {
        Insert::new(self.clone())
    }

    /// Starts an `UPDATE` for a generated table.
    pub fn update<T: crate::metadata::Table>(&self) -> Update<D, T> {
        Update::new(self.clone())
    }

    /// Starts a `DELETE` for a generated table.
    pub fn delete<T: crate::metadata::Table>(&self) -> Delete<D, T> {
        Delete::new(self.clone())
    }

    /// Starts an upsert for a generated table.
    ///
    /// Available only for dialects that implement [`SupportsUpsert`].
    pub fn upsert<T: crate::metadata::Table>(&self) -> Upsert<D, T>
    where
        D: SupportsUpsert,
    {
        Upsert::new(self.clone())
    }

    /// Starts a raw SQL query. The caller is responsible for the SQL text.
    pub fn sql(&self, sql: impl Into<String>) -> RawQuery<'_, D> {
        RawQuery {
            db: self,
            sql: sql.into(),
            arguments: <D::DB as Database>::Arguments::default(),
            error: None,
        }
    }

    /// Starts a transaction on a pooled connection.
    pub async fn begin(&self) -> Result<Transaction<'_, D>> {
        Ok(Transaction {
            db: self,
            inner: self.pool.begin().await?,
        })
    }
}

/// An in-progress transaction bound to a [`Db`].
///
/// Queries built with [`Db::query`], [`Db::insert`], [`Db::update`], and
/// [`Db::delete`] execute inside the transaction with their `exec_in` methods.
/// The transaction rolls back on drop unless committed.
pub struct Transaction<'a, D: Dialect + Clone> {
    db: &'a Db<D>,
    inner: SqlxTransaction<'static, D::DB>,
}

impl<'a, D: Dialect + Clone> Transaction<'a, D> {
    /// Returns the database handle.
    pub fn db(&self) -> &Db<D> {
        self.db
    }

    /// The underlying sqlx transaction, for SQLx APIs that need an executor.
    pub fn inner(&mut self) -> &mut SqlxTransaction<'static, D::DB> {
        &mut self.inner
    }

    /// The transaction's connection, usable as a SQLx executor.
    pub fn connection(&mut self) -> &mut <D::DB as Database>::Connection {
        &mut self.inner
    }

    /// Starts a typed query.
    pub fn query<Q>(&self, source: Q) -> Select<D, Q::Table>
    where
        Q: IntoSelect<D>,
    {
        self.db.query(source)
    }

    /// Starts an `INSERT`.
    pub fn insert<T: crate::metadata::Table>(&self) -> Insert<D, T> {
        Insert::new(self.db.clone())
    }

    /// Starts an `UPDATE`.
    pub fn update<T: crate::metadata::Table>(&self) -> Update<D, T> {
        Update::new(self.db.clone())
    }

    /// Starts a `DELETE`.
    pub fn delete<T: crate::metadata::Table>(&self) -> Delete<D, T> {
        Delete::new(self.db.clone())
    }

    /// Starts an upsert.
    ///
    /// Available only for dialects that implement [`SupportsUpsert`].
    pub fn upsert<T: crate::metadata::Table>(&self) -> Upsert<D, T>
    where
        D: SupportsUpsert,
    {
        Upsert::new(self.db.clone())
    }

    /// Starts a raw SQL query.
    pub fn sql(&self, sql: impl Into<String>) -> RawQuery<'_, D> {
        self.db.sql(sql)
    }

    /// Commits the transaction.
    pub async fn commit(self) -> Result<()> {
        self.inner.commit().await
    }

    /// Rolls back the transaction.
    pub async fn rollback(self) -> Result<()> {
        self.inner.rollback().await
    }
}

/// A raw SQL query with bound arguments.
pub struct RawQuery<'a, D: Dialect + Clone> {
    db: &'a Db<D>,
    sql: String,
    arguments: <D::DB as Database>::Arguments,
    error: Option<BoxDynError>,
}

impl<D: Dialect + Clone> RawQuery<'_, D> {
    /// Binds one argument.
    pub fn bind<T>(mut self, value: T) -> Self
    where
        T: for<'q> Encode<'q, D::DB> + Type<D::DB>,
    {
        if self.error.is_none()
            && let Err(error) = self.arguments.add(value)
        {
            self.error = Some(error);
        }
        self
    }

    /// Fetches all rows.
    pub async fn fetch_all<T>(mut self) -> Result<Vec<T>>
    where
        T: Send + Unpin + for<'r> FromRow<'r, <D::DB as Database>::Row>,
        <D::DB as Database>::Arguments: sqlx_core::arguments::IntoArguments<D::DB>,
        for<'c> &'c mut <D::DB as Database>::Connection: Executor<'c, Database = D::DB>,
    {
        let sql = self.take_sql()?;
        query_as_with::<D::DB, T, _>(sql, self.arguments)
            .fetch_all(self.db.pool())
            .await
    }

    /// Fetches exactly one row.
    pub async fn fetch_one<T>(mut self) -> Result<T>
    where
        T: Send + Unpin + for<'r> FromRow<'r, <D::DB as Database>::Row>,
        <D::DB as Database>::Arguments: sqlx_core::arguments::IntoArguments<D::DB>,
        for<'c> &'c mut <D::DB as Database>::Connection: Executor<'c, Database = D::DB>,
    {
        let sql = self.take_sql()?;
        query_as_with::<D::DB, T, _>(sql, self.arguments)
            .fetch_one(self.db.pool())
            .await
    }

    /// Fetches zero or one row.
    pub async fn fetch_optional<T>(mut self) -> Result<Option<T>>
    where
        T: Send + Unpin + for<'r> FromRow<'r, <D::DB as Database>::Row>,
        <D::DB as Database>::Arguments: sqlx_core::arguments::IntoArguments<D::DB>,
        for<'c> &'c mut <D::DB as Database>::Connection: Executor<'c, Database = D::DB>,
    {
        let sql = self.take_sql()?;
        query_as_with::<D::DB, T, _>(sql, self.arguments)
            .fetch_optional(self.db.pool())
            .await
    }

    /// Executes the statement.
    pub async fn execute(mut self) -> Result<<D::DB as Database>::QueryResult>
    where
        <D::DB as Database>::Arguments: sqlx_core::arguments::IntoArguments<D::DB>,
        for<'c> &'c mut <D::DB as Database>::Connection: Executor<'c, Database = D::DB>,
    {
        let sql = self.take_sql()?;
        query_with::<D::DB, _>(sql, self.arguments)
            .execute(self.db.pool())
            .await
    }

    fn take_sql(&mut self) -> Result<sqlx_core::sql_str::SqlStr> {
        if let Some(error) = self.error.take() {
            return Err(Error::Encode(error));
        }

        Ok(AssertSqlSafe(std::mem::take(&mut self.sql)).into_sql_str())
    }
}
