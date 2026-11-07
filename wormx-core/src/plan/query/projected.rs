use crate::plan::command::Writer;
use crate::plan::expr::{Expr, IntoExpr, Order};

use sqlx_core::database::Database;
use sqlx_core::error::{Error, Result};
use sqlx_core::executor::Executor;
use sqlx_core::from_row::FromRow;
use sqlx_core::query_as::query_as_with;
use sqlx_core::sql_str::{AssertSqlSafe, SqlSafeStr};

use super::column::Column;
use super::cte::{Cte, Subquery};
use super::projection::{CountProjection, Projection};
use super::state::SelectState;
use crate::plan::backend::{Dialect, SupportsSourceSuffix, SupportsStatementTail};
use crate::plan::join::Join;
use crate::schema::Table;

/// A projected table query bound to a dialect and database handle.
pub struct ProjectedSelect<D: Dialect + Clone, T: Table + 'static, P: Projection<D>> {
    pub(super) state: SelectState<D, T>,
    pub(super) projection: P,
    pub(super) groups: Vec<Expr<(), D>>,
    pub(super) having: Vec<Expr<bool, D>>,
    pub(super) dropped_includes: bool,
}

/// Single-column shorthand.
pub type ColumnSelect<D, T, C> = ProjectedSelect<D, T, Column<C>>;

impl<D, T, P> ProjectedSelect<D, T, P>
where
    D: Dialect + Clone,
    T: Table + 'static,
    P: Projection<D>,
{
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

    /// Adds a grouping expression.
    pub fn group_by<E, V>(mut self, expression: V) -> Self
    where
        V: IntoExpr<E, D>,
    {
        self.groups.push(expression.into_expr().erase());
        self
    }

    /// Adds a `HAVING` predicate.
    pub fn having(mut self, filter: Expr<bool, D>) -> Self {
        self.having.push(filter);
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

    fn ensure_no_dropped_includes(&self) -> Result<()> {
        if self.dropped_includes {
            return Err(Error::InvalidArgument(
                "eager loading cannot be applied after .select(...); add .include(...) to the row query instead"
                    .to_owned(),
            ));
        }
        Ok(())
    }

    /// Executes the query and returns the first row.
    pub async fn first(mut self) -> Result<Option<P::Output>>
    where
        P::Row: for<'r> FromRow<'r, <D::DB as Database>::Row>,
        <D::DB as Database>::Arguments: sqlx_core::arguments::IntoArguments<D::DB>,
        for<'c> &'c mut <D::DB as Database>::Connection: Executor<'c, Database = D::DB>,
    {
        self.state.limit = Some(1);
        let mut rows = self.exec().await?;
        Ok(rows.pop())
    }

    /// Renders this query with the dialect's renderer without executing it.
    pub fn render(&mut self) -> Result<(String, <D::DB as Database>::Arguments)> {
        let mut writer = self.state.db.writer();
        self.state.write(
            &mut self.projection,
            &mut self.groups,
            &mut self.having,
            &mut writer,
        )?;
        Ok(writer.finish())
    }

    /// Executes the query.
    pub async fn exec(mut self) -> Result<Vec<P::Output>>
    where
        P::Row: for<'r> FromRow<'r, <D::DB as Database>::Row>,
        <D::DB as Database>::Arguments: sqlx_core::arguments::IntoArguments<D::DB>,
        for<'c> &'c mut <D::DB as Database>::Connection: Executor<'c, Database = D::DB>,
    {
        self.ensure_no_dropped_includes()?;
        let (sql, arguments) = self.render()?;
        let rows: Vec<P::Row> =
            query_as_with::<D::DB, P::Row, _>(AssertSqlSafe(sql).into_sql_str(), arguments)
                .fetch_all(self.state.db.pool())
                .await?;

        Ok(rows.into_iter().map(P::map).collect())
    }

    /// Executes the query inside a transaction.
    pub async fn exec_in(
        mut self,
        tx: &mut crate::exec::Transaction<'_, D>,
    ) -> Result<Vec<P::Output>>
    where
        P::Row: for<'r> FromRow<'r, <D::DB as Database>::Row>,
        <D::DB as Database>::Arguments: sqlx_core::arguments::IntoArguments<D::DB>,
        for<'c> &'c mut <D::DB as Database>::Connection: Executor<'c, Database = D::DB>,
    {
        self.ensure_no_dropped_includes()?;
        let (sql, arguments) = self.render()?;
        let rows: Vec<P::Row> =
            query_as_with::<D::DB, P::Row, _>(AssertSqlSafe(sql).into_sql_str(), arguments)
                .fetch_all(tx.connection())
                .await?;

        Ok(rows.into_iter().map(P::map).collect())
    }
}

impl<D, T, P> ProjectedSelect<D, T, P>
where
    D: Dialect + Clone,
    T: Table + 'static,
    P: CountProjection<D>,
{
    /// Executes `SELECT COUNT(projection) ...`.
    pub async fn count(mut self) -> Result<u64>
    where
        usize: sqlx_core::column::ColumnIndex<<D::DB as Database>::Row>,
        i64: for<'q> sqlx_core::decode::Decode<'q, D::DB> + sqlx_core::types::Type<D::DB>,
        <D::DB as Database>::Arguments: sqlx_core::arguments::IntoArguments<D::DB>,
        for<'c> &'c mut <D::DB as Database>::Connection: Executor<'c, Database = D::DB>,
    {
        self.count_impl(false).await
    }

    /// Executes `SELECT COUNT(DISTINCT projection) ...`.
    pub async fn count_distinct(mut self) -> Result<u64>
    where
        usize: sqlx_core::column::ColumnIndex<<D::DB as Database>::Row>,
        i64: for<'q> sqlx_core::decode::Decode<'q, D::DB> + sqlx_core::types::Type<D::DB>,
        <D::DB as Database>::Arguments: sqlx_core::arguments::IntoArguments<D::DB>,
        for<'c> &'c mut <D::DB as Database>::Connection: Executor<'c, Database = D::DB>,
    {
        self.count_impl(true).await
    }

    async fn count_impl(&mut self, distinct: bool) -> Result<u64>
    where
        usize: sqlx_core::column::ColumnIndex<<D::DB as Database>::Row>,
        i64: for<'q> sqlx_core::decode::Decode<'q, D::DB> + sqlx_core::types::Type<D::DB>,
        <D::DB as Database>::Arguments: sqlx_core::arguments::IntoArguments<D::DB>,
        for<'c> &'c mut <D::DB as Database>::Connection: Executor<'c, Database = D::DB>,
    {
        self.ensure_no_dropped_includes()?;
        let mut writer = self.state.db.writer();
        self.state.write_count(
            &mut self.projection,
            &mut self.groups,
            &mut self.having,
            &mut writer,
            distinct,
        )?;

        let (sql, arguments) = writer.finish();
        let (count,): (i64,) =
            query_as_with::<D::DB, (i64,), _>(AssertSqlSafe(sql).into_sql_str(), arguments)
                .fetch_one(self.state.db.pool())
                .await?;
        Ok(count as u64)
    }
}

impl<D, T, P> Subquery<D> for ProjectedSelect<D, T, P>
where
    D: Dialect + Clone,
    T: Table + 'static,
    P: Projection<D>,
{
    fn render(&mut self, writer: &mut Writer<D>) -> Result<()> {
        self.state.write(
            &mut self.projection,
            &mut self.groups,
            &mut self.having,
            writer,
        )
    }
}
