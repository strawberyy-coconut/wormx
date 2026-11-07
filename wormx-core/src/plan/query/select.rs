use sqlx_core::database::Database;
use sqlx_core::error::Result;
use sqlx_core::executor::Executor;
use sqlx_core::from_row::FromRow;
use sqlx_core::query_as::query_as_with;
use sqlx_core::sql_str::{AssertSqlSafe, SqlSafeStr};

use super::cte::Cte;
use super::projected::ProjectedSelect;
use super::projection::{Projection, StarProjection};
use super::state::SelectState;
use crate::exec::Db;
use crate::plan::backend::{Dialect, SupportsSourceSuffix, SupportsStatementTail};
use crate::plan::expr::{Expr, Order};
use crate::plan::include::Include;
use crate::plan::join::Join;
use crate::schema::Table;

pub struct Select<D: Dialect + Clone, T: 'static> {
    state: SelectState<D, T>,
    includes: Vec<Box<dyn Include<D, T>>>,
}

impl<D: Dialect + Clone, T: Table + 'static> Select<D, T> {
    pub(super) fn new(db: Db<D>) -> Self {
        Self {
            state: SelectState::new(db),
            includes: Vec::new(),
        }
    }

    /// Adds a filter predicate.
    pub fn filter(mut self, filter: Expr<bool, D>) -> Self {
        self.state = self.state.filter(filter);
        self
    }

    /// Selects distinct rows.
    pub fn distinct(mut self) -> Self {
        self.state = self.state.distinct();
        self
    }

    /// Adds a common table expression before the query.
    pub fn with(mut self, cte: Cte<D>) -> Self {
        self.state = self.state.with(cte);
        self
    }

    /// Adds a join.
    pub fn join(mut self, join: Join<D>) -> Self {
        self.state = self.state.join(join);
        self
    }

    /// Adds an order term.
    pub fn order_by(mut self, order: Order<D>) -> Self {
        self.state = self.state.order_by(order);
        self
    }

    /// Sets the maximum number of rows.
    pub fn limit(mut self, limit: u64) -> Self {
        self.state = self.state.limit(limit);
        self
    }

    /// Skips the given number of rows.
    pub fn offset(mut self, offset: u64) -> Self {
        self.state = self.state.offset(offset);
        self
    }

    /// Attaches an opaque, dialect-defined suffix to the base table.
    pub fn with_source_suffix(mut self, suffix: &'static str) -> Self
    where
        D: SupportsSourceSuffix,
    {
        self.state = self.state.with_source_suffix(suffix);
        self
    }

    /// Attaches an opaque, dialect-defined tail to the statement.
    pub fn with_tail(mut self, tail: &'static str) -> Self
    where
        D: SupportsStatementTail,
    {
        self.state = self.state.with_tail(tail);
        self
    }

    /// Projects the query into a typed result shape.
    pub fn select<P>(self, projection: P) -> ProjectedSelect<D, T, P>
    where
        P: Projection<D>,
    {
        let Self { state, includes } = self;
        let dropped_includes = !includes.is_empty();
        ProjectedSelect {
            state,
            projection,
            groups: Vec::new(),
            having: Vec::new(),
            dropped_includes,
        }
    }

    /// Adds an eager-loading step.
    pub fn include<I>(mut self, include: I) -> Self
    where
        I: Include<D, T> + 'static,
    {
        self.includes.push(Box::new(include));
        self
    }

    /// Renders this query with the dialect's renderer without executing it.
    pub fn render(&mut self) -> Result<(String, <D::DB as Database>::Arguments)> {
        let mut writer = self.state.db.writer();
        let mut groups = [];
        let mut having = [];
        self.state
            .write(&mut StarProjection, &mut groups, &mut having, &mut writer)?;
        Ok(writer.finish())
    }

    /// Executes the query.
    pub async fn exec(mut self) -> Result<Vec<T>>
    where
        T: Table + Send + Unpin + for<'r> FromRow<'r, <D::DB as Database>::Row>,
        <D::DB as Database>::Arguments: sqlx_core::arguments::IntoArguments<D::DB>,
        for<'c> &'c mut <D::DB as Database>::Connection: Executor<'c, Database = D::DB>,
    {
        let (sql, arguments) = self.render()?;
        let mut rows: Vec<T> =
            query_as_with::<D::DB, T, _>(AssertSqlSafe(sql).into_sql_str(), arguments)
                .fetch_all(self.state.db.pool())
                .await?;

        for include in &self.includes {
            include.apply(&self.state.db, &mut rows).await?;
        }

        Ok(rows)
    }

    /// Executes the query inside a transaction.
    pub async fn exec_in(mut self, tx: &mut crate::exec::Transaction<'_, D>) -> Result<Vec<T>>
    where
        T: Table + Send + Unpin + for<'r> FromRow<'r, <D::DB as Database>::Row>,
        <D::DB as Database>::Arguments: sqlx_core::arguments::IntoArguments<D::DB>,
        for<'c> &'c mut <D::DB as Database>::Connection: Executor<'c, Database = D::DB>,
    {
        let (sql, arguments) = self.render()?;
        let mut rows: Vec<T> =
            query_as_with::<D::DB, T, _>(AssertSqlSafe(sql).into_sql_str(), arguments)
                .fetch_all(tx.connection())
                .await?;

        for include in &self.includes {
            include.apply_in(&self.state.db, tx, &mut rows).await?;
        }

        Ok(rows)
    }

    /// Executes `SELECT COUNT(*)`.
    pub async fn count(mut self) -> Result<u64>
    where
        T: Table,
        usize: sqlx_core::column::ColumnIndex<<D::DB as Database>::Row>,
        i64: for<'q> sqlx_core::decode::Decode<'q, D::DB> + sqlx_core::types::Type<D::DB>,
        <D::DB as Database>::Arguments: sqlx_core::arguments::IntoArguments<D::DB>,
        for<'c> &'c mut <D::DB as Database>::Connection: Executor<'c, Database = D::DB>,
    {
        let mut writer = self.state.db.writer();
        self.state.write_count_star(&mut writer)?;

        let (sql, arguments) = writer.finish();
        let (count,): (i64,) =
            query_as_with::<D::DB, (i64,), _>(AssertSqlSafe(sql).into_sql_str(), arguments)
                .fetch_one(self.state.db.pool())
                .await?;

        Ok(count as u64)
    }

    /// Executes the query and returns the first row.
    pub async fn first(mut self) -> Result<Option<T>>
    where
        T: Table + Send + Unpin + for<'r> FromRow<'r, <D::DB as Database>::Row>,
        <D::DB as Database>::Arguments: sqlx_core::arguments::IntoArguments<D::DB>,
        for<'c> &'c mut <D::DB as Database>::Connection: Executor<'c, Database = D::DB>,
    {
        self.state.limit = Some(1);
        let mut rows = self.exec().await?;
        Ok(rows.pop())
    }
}
