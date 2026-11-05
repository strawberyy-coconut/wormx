use std::marker::PhantomData;

use sqlx_core::database::Database;
use sqlx_core::encode::Encode;
use sqlx_core::error::Result;
use sqlx_core::executor::Executor;
use sqlx_core::from_row::FromRow;
use sqlx_core::query_as::query_as_with;
use sqlx_core::sql_str::{AssertSqlSafe, SqlSafeStr};
use sqlx_core::types::Type;

use crate::db::Db;
use crate::dialect::{Dialect, Expr, IntoExpr, Order, SqlWriter};
use crate::include::Include;
use crate::join::{Join, push_table};
use crate::metadata::{Table, TableDef};

/// A dialect-neutral table query.
pub struct Query<T> {
    _marker: PhantomData<fn() -> T>,
}

impl<T> Query<T> {
    pub const fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<T> Default for Query<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// A lookup by primary key.
pub struct Get<T, K> {
    key: K,
    _marker: PhantomData<fn() -> T>,
}

impl<T, K> Get<T, K> {
    pub const fn new(key: K) -> Self {
        Self {
            key,
            _marker: PhantomData,
        }
    }
}

/// A table query with a filter, independent of a database handle.
pub struct Filtered<D: Dialect + Clone, T: Table + 'static> {
    filter: Expr<bool, D>,
    _marker: PhantomData<fn() -> T>,
}

impl<D: Dialect + Clone, T: Table + 'static> Filtered<D, T> {
    pub fn new(filter: Expr<bool, D>) -> Self {
        Self {
            filter,
            _marker: PhantomData,
        }
    }
}

impl<D: Dialect + Clone, T: Table + 'static> IntoSelect<D> for Filtered<D, T> {
    type Table = T;

    fn into_select(self, db: &Db<D>) -> Select<D, T> {
        db.query(Query::<T>::new()).filter(self.filter)
    }
}

/// Writes the SQL for a projection.
///
/// This is separate from [`Projection`] so a renderer can write a projection
/// without knowing how result rows are mapped back into Rust values.
pub trait ProjectionWriter<D: Dialect>: Send {
    fn write(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<()>;
}

/// The `*` projection used by generated table selects.
#[derive(Debug, Clone, Copy, Default)]
pub struct StarProjection;

impl<D: Dialect> ProjectionWriter<D> for StarProjection {
    fn write(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<()> {
        writer.write_star();
        Ok(())
    }
}

/// A selectable projection: one column/expression, or a tuple of them.
pub trait Projection<D: Dialect>: ProjectionWriter<D> {
    type Output;
    type Row: Send + Unpin;

    fn map(row: Self::Row) -> Self::Output;
}

impl<D: Dialect, C: Send + Unpin> ProjectionWriter<D> for Column<C> {
    fn write(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<()> {
        self.render_column(writer);
        Ok(())
    }
}

impl<D: Dialect, C: Send + Unpin> Projection<D> for Column<C> {
    type Output = C;
    type Row = Scalar<C>;

    fn map(row: Self::Row) -> Self::Output {
        row.0
    }
}

impl<D: Dialect, C: Send + Unpin> ProjectionWriter<D> for Expr<C, D> {
    fn write(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<()> {
        self.render(writer)
    }
}

impl<D: Dialect, C: Send + Unpin> Projection<D> for Expr<C, D> {
    type Output = C;
    type Row = Scalar<C>;

    fn map(row: Self::Row) -> Self::Output {
        row.0
    }
}

// Tuple field access by index is not possible in a macro_rules! repetition, so
// the tuple impls are written explicitly below.
impl<D, A, B> ProjectionWriter<D> for (A, B)
where
    D: Dialect,
    A: ProjectionWriter<D>,
    B: ProjectionWriter<D>,
{
    fn write(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<()> {
        self.0.write(writer)?;
        writer.write_comma();
        self.1.write(writer)
    }
}

impl<D, A, B> Projection<D> for (A, B)
where
    D: Dialect,
    A: Projection<D>,
    B: Projection<D>,
    A::Output: Send + Unpin,
    B::Output: Send + Unpin,
{
    type Output = (A::Output, B::Output);
    type Row = (A::Output, B::Output);

    fn map(row: Self::Row) -> Self::Output {
        row
    }
}

impl<D, A, B, C> ProjectionWriter<D> for (A, B, C)
where
    D: Dialect,
    A: ProjectionWriter<D>,
    B: ProjectionWriter<D>,
    C: ProjectionWriter<D>,
{
    fn write(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<()> {
        self.0.write(writer)?;
        writer.write_comma();
        self.1.write(writer)?;
        writer.write_comma();
        self.2.write(writer)
    }
}

impl<D, A, B, C> Projection<D> for (A, B, C)
where
    D: Dialect,
    A: Projection<D>,
    B: Projection<D>,
    C: Projection<D>,
    A::Output: Send + Unpin,
    B::Output: Send + Unpin,
    C::Output: Send + Unpin,
{
    type Output = (A::Output, B::Output, C::Output);
    type Row = (A::Output, B::Output, C::Output);

    fn map(row: Self::Row) -> Self::Output {
        row
    }
}

/// A projected table query bound to a dialect and database handle.
pub struct ProjectedSelect<D: Dialect + Clone, T: Table + 'static, P: Projection<D>> {
    db: Db<D>,
    distinct: bool,
    filters: Vec<Expr<bool, D>>,
    joins: Vec<Join<D>>,
    orders: Vec<Order<D>>,
    limit: Option<u64>,
    offset: Option<u64>,
    projection: P,
    groups: Vec<Expr<(), D>>,
    having: Vec<Expr<bool, D>>,
    ctes: Vec<Cte<D>>,
    _source: PhantomData<fn() -> T>,
}

/// Single-column shorthand.
pub type ColumnSelect<D, T, C> = ProjectedSelect<D, T, Column<C>>;

/// A subquery that can be rendered inside an expression.
pub trait Subquery<D: Dialect>: Send {
    fn render(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<()>;
}

/// A common table expression: `name AS (query)`.
pub struct Cte<D: Dialect> {
    name: String,
    query: Box<dyn Subquery<D>>,
}

impl<D: Dialect> Cte<D> {
    pub fn new(name: impl Into<String>, query: impl Subquery<D> + 'static) -> Self {
        Self {
            name: name.into(),
            query: Box::new(query),
        }
    }

    pub fn render(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<()> {
        writer.push_ident(&self.name);
        writer.push(" AS (");
        self.query.render(writer)?;
        writer.push(")");
        Ok(())
    }
}

pub(crate) fn write_ctes<D: Dialect>(
    writer: &mut SqlWriter<'_, D>,
    ctes: &mut [Cte<D>],
) -> Result<()> {
    if ctes.is_empty() {
        return Ok(());
    }

    writer.push("WITH ");
    for (index, cte) in ctes.iter_mut().enumerate() {
        if index > 0 {
            writer.write_comma();
        }
        cte.render(writer)?;
    }
    writer.write_space();

    Ok(())
}

impl<D, T, P> ProjectedSelect<D, T, P>
where
    D: Dialect + Clone,
    T: Table + 'static,
    P: Projection<D>,
{
    pub fn filter(mut self, filter: Expr<bool, D>) -> Self {
        self.filters.push(filter);
        self
    }

    pub fn distinct(mut self) -> Self {
        self.distinct = true;
        self
    }

    /// Adds a common table expression before the query.
    pub fn with(mut self, cte: Cte<D>) -> Self {
        self.ctes.push(cte);
        self
    }

    pub fn join(mut self, join: Join<D>) -> Self {
        self.joins.push(join);
        self
    }

    pub fn group_by<E, V>(mut self, expression: V) -> Self
    where
        V: IntoExpr<E, D>,
    {
        self.groups.push(expression.into_expr().erase());
        self
    }

    pub fn having(mut self, filter: Expr<bool, D>) -> Self {
        self.having.push(filter);
        self
    }

    pub fn order_by(mut self, order: Order<D>) -> Self {
        self.orders.push(order);
        self
    }

    pub fn limit(mut self, limit: u64) -> Self {
        self.limit = Some(limit);
        self
    }

    pub fn offset(mut self, offset: u64) -> Self {
        self.offset = Some(offset);
        self
    }

    pub async fn first(mut self) -> Result<Option<P::Output>>
    where
        P::Row: for<'r> FromRow<'r, <D::DB as Database>::Row>,
        <D::DB as Database>::Arguments: sqlx_core::arguments::IntoArguments<D::DB>,
        for<'c> &'c mut <D::DB as Database>::Connection: Executor<'c, Database = D::DB>,
    {
        self.limit = Some(1);
        let mut rows = self.exec().await?;
        Ok(rows.pop())
    }

    pub async fn exec(mut self) -> Result<Vec<P::Output>>
    where
        P::Row: for<'r> FromRow<'r, <D::DB as Database>::Row>,
        <D::DB as Database>::Arguments: sqlx_core::arguments::IntoArguments<D::DB>,
        for<'c> &'c mut <D::DB as Database>::Connection: Executor<'c, Database = D::DB>,
    {
        let mut writer = SqlWriter::new(self.db.dialect());
        write_ctes(&mut writer, &mut self.ctes)?;
        D::Select::default().write(
            SelectPlan {
                table: T::DEF,
                distinct: self.distinct,
                projection: &mut self.projection,
                filters: &mut self.filters,
                joins: &mut self.joins,
                groups: &mut self.groups,
                having: &mut self.having,
                orders: &mut self.orders,
                limit: self.limit,
                offset: self.offset,
            },
            &mut writer,
        )?;

        let (sql, arguments) = writer.finish();
        let rows: Vec<P::Row> =
            query_as_with::<D::DB, P::Row, _>(AssertSqlSafe(sql).into_sql_str(), arguments)
                .fetch_all(self.db.pool())
                .await?;

        Ok(rows.into_iter().map(P::map).collect())
    }

    pub async fn exec_in(mut self, tx: &mut crate::db::Transaction<'_, D>) -> Result<Vec<P::Output>>
    where
        P::Row: for<'r> FromRow<'r, <D::DB as Database>::Row>,
        <D::DB as Database>::Arguments: sqlx_core::arguments::IntoArguments<D::DB>,
        for<'c> &'c mut <D::DB as Database>::Connection: Executor<'c, Database = D::DB>,
    {
        let mut writer = SqlWriter::new(self.db.dialect());
        write_ctes(&mut writer, &mut self.ctes)?;
        D::Select::default().write(
            SelectPlan {
                table: T::DEF,
                distinct: self.distinct,
                projection: &mut self.projection,
                filters: &mut self.filters,
                joins: &mut self.joins,
                groups: &mut self.groups,
                having: &mut self.having,
                orders: &mut self.orders,
                limit: self.limit,
                offset: self.offset,
            },
            &mut writer,
        )?;

        let (sql, arguments) = writer.finish();
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
        let mut writer = SqlWriter::new(self.db.dialect());
        write_ctes(&mut writer, &mut self.ctes)?;
        D::Select::default().write_count(
            SelectPlan {
                table: T::DEF,
                distinct: self.distinct,
                projection: &mut self.projection,
                filters: &mut self.filters,
                joins: &mut self.joins,
                groups: &mut self.groups,
                having: &mut self.having,
                orders: &mut self.orders,
                limit: self.limit,
                offset: self.offset,
            },
            &mut writer,
            distinct,
        )?;

        let (sql, arguments) = writer.finish();
        let (count,): (i64,) =
            query_as_with::<D::DB, (i64,), _>(AssertSqlSafe(sql).into_sql_str(), arguments)
                .fetch_one(self.db.pool())
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
    fn render(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<()> {
        D::Select::default().write(
            SelectPlan {
                table: T::DEF,
                distinct: self.distinct,
                projection: &mut self.projection,
                filters: &mut self.filters,
                joins: &mut self.joins,
                groups: &mut self.groups,
                having: &mut self.having,
                orders: &mut self.orders,
                limit: self.limit,
                offset: self.offset,
            },
            writer,
        )
    }
}

/// Projections that support `COUNT(expr)` and `COUNT(DISTINCT expr)`.
pub trait CountProjection<D: Dialect>: Projection<D> {
    fn write_count(&mut self, writer: &mut SqlWriter<'_, D>, distinct: bool) -> Result<()>;
}

impl<D: Dialect, C: Send + Unpin> CountProjection<D> for Column<C> {
    fn write_count(&mut self, writer: &mut SqlWriter<'_, D>, distinct: bool) -> Result<()> {
        writer.write_count_start(distinct);
        self.write(writer)?;
        writer.write_group_end();
        Ok(())
    }
}

impl<D: Dialect, C: Send + Unpin> CountProjection<D> for Expr<C, D> {
    fn write_count(&mut self, writer: &mut SqlWriter<'_, D>, distinct: bool) -> Result<()> {
        writer.write_count_start(distinct);
        self.write(writer)?;
        writer.write_group_end();
        Ok(())
    }
}

/// All inputs needed to render a `SELECT` operation.
pub struct SelectPlan<'a, D: Dialect, P> {
    pub table: &'static TableDef,
    pub distinct: bool,
    pub projection: &'a mut P,
    pub filters: &'a mut [Expr<bool, D>],
    pub joins: &'a mut [Join<D>],
    pub groups: &'a mut [Expr<(), D>],
    pub having: &'a mut [Expr<bool, D>],
    pub orders: &'a mut [Order<D>],
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

/// Inputs needed to render `SELECT COUNT(*)`.
pub struct CountPlan<'a, D: Dialect> {
    pub table: &'static TableDef,
    pub distinct: bool,
    pub filters: &'a mut [Expr<bool, D>],
    pub joins: &'a mut [Join<D>],
}

/// Renders the `SELECT` operation for a dialect.
///
/// A dialect chooses its renderer with [`Dialect::Select`].
pub trait SelectRenderer<D: Dialect>: Default {
    /// Renders a row-producing select.
    fn write<P>(&self, plan: SelectPlan<'_, D, P>, writer: &mut SqlWriter<'_, D>) -> Result<()>
    where
        P: ProjectionWriter<D>;

    /// Renders `SELECT COUNT(projection)` (or `COUNT(DISTINCT projection)`).
    fn write_count<P>(
        &self,
        plan: SelectPlan<'_, D, P>,
        writer: &mut SqlWriter<'_, D>,
        distinct: bool,
    ) -> Result<()>
    where
        P: ProjectionWriter<D> + CountProjection<D>;

    /// Renders `SELECT COUNT(*)`.
    fn write_count_star(&self, plan: CountPlan<'_, D>, writer: &mut SqlWriter<'_, D>)
    -> Result<()>;
}

/// The generic SQL renderer for [`SelectRenderer`].
#[derive(Debug, Clone, Copy, Default)]
pub struct SqlSelect;

impl<D: Dialect> SelectRenderer<D> for SqlSelect {
    fn write<P>(&self, plan: SelectPlan<'_, D, P>, writer: &mut SqlWriter<'_, D>) -> Result<()>
    where
        P: ProjectionWriter<D>,
    {
        writer.write_select(plan.distinct);
        plan.projection.write(writer)?;
        write_from_filters(writer, plan.table, plan.filters, plan.joins)?;
        write_groups_having(writer, plan.groups, plan.having)?;
        write_orders(writer, plan.orders)?;
        writer.push_limit_offset(!plan.orders.is_empty(), plan.limit, plan.offset);
        Ok(())
    }

    fn write_count<P>(
        &self,
        plan: SelectPlan<'_, D, P>,
        writer: &mut SqlWriter<'_, D>,
        distinct: bool,
    ) -> Result<()>
    where
        P: ProjectionWriter<D> + CountProjection<D>,
    {
        writer.write_select(false);
        plan.projection.write_count(writer, distinct)?;
        write_from_filters(writer, plan.table, plan.filters, plan.joins)
    }

    fn write_count_star(
        &self,
        plan: CountPlan<'_, D>,
        writer: &mut SqlWriter<'_, D>,
    ) -> Result<()> {
        writer.write_select(plan.distinct);
        writer.write_count_star();
        write_from_filters(writer, plan.table, plan.filters, plan.joins)
    }
}

/// All inputs needed to render an `INSERT` operation.
pub struct InsertPlan<'a, D: Dialect> {
    pub table: &'static TableDef,
    pub columns: &'a [&'static str],
    pub values: &'a mut [Expr<(), D>],
    pub returning: bool,
}

/// All inputs needed to render an `UPDATE` operation.
pub struct UpdatePlan<'a, D: Dialect> {
    pub table: &'static TableDef,
    pub assignments: &'a mut [(&'static str, Expr<(), D>)],
    pub filters: &'a mut [Expr<bool, D>],
    pub returning: bool,
}

/// All inputs needed to render a `DELETE` operation.
pub struct DeletePlan<'a, D: Dialect> {
    pub table: &'static TableDef,
    pub filters: &'a mut [Expr<bool, D>],
    pub returning: bool,
}

/// All inputs needed to render an `UPSERT`/`MERGE` operation.
pub struct UpsertPlan<'a, D: Dialect> {
    pub table: &'static TableDef,
    pub columns: &'a [&'static str],
    pub values: &'a mut [Expr<(), D>],
    pub conflict_columns: &'a [&'static str],
    pub assignments: &'a mut [(&'static str, Expr<(), D>)],
    pub returning: bool,
}

/// Renders an `INSERT` operation.
///
/// A dialect chooses this renderer with [`Dialect::Insert`].
pub trait InsertRenderer<D: Dialect>: Default {
    fn write(&self, plan: InsertPlan<'_, D>, writer: &mut SqlWriter<'_, D>) -> Result<()>;
}

/// Renders an `UPDATE` operation.
///
/// A dialect chooses this renderer with [`Dialect::Update`].
pub trait UpdateRenderer<D: Dialect>: Default {
    fn write(&self, plan: UpdatePlan<'_, D>, writer: &mut SqlWriter<'_, D>) -> Result<()>;
}

/// Renders a `DELETE` operation.
///
/// A dialect chooses this renderer with [`Dialect::Delete`].
pub trait DeleteRenderer<D: Dialect>: Default {
    fn write(&self, plan: DeletePlan<'_, D>, writer: &mut SqlWriter<'_, D>) -> Result<()>;
}

/// Renders an `UPSERT`/`MERGE` operation.
///
/// A dialect chooses this renderer with [`Dialect::Upsert`].
pub trait UpsertRenderer<D: Dialect>: Default {
    fn write(&self, plan: UpsertPlan<'_, D>, writer: &mut SqlWriter<'_, D>) -> Result<()>;
}

/// The generic SQL `INSERT` renderer.
#[derive(Debug, Clone, Copy, Default)]
pub struct SqlInsert;

impl<D: Dialect> InsertRenderer<D> for SqlInsert {
    fn write(&self, plan: InsertPlan<'_, D>, writer: &mut SqlWriter<'_, D>) -> Result<()> {
        writer.write_insert();
        push_table(writer, plan.table.schema, plan.table.name);

        if plan.columns.is_empty() {
            writer.write_insert_default();
        } else {
            writer.write_group_start();
            for (index, column) in plan.columns.iter().enumerate() {
                if index > 0 {
                    writer.write_comma();
                }
                writer.push_ident(column);
            }
            writer.write_group_end();
            writer.write_values();

            for (index, value) in plan.values.iter_mut().enumerate() {
                if index > 0 {
                    writer.write_comma();
                }
                value.render(writer)?;
            }
            writer.write_group_end();
        }

        if plan.returning {
            writer.write_returning();
        }

        Ok(())
    }
}

/// The generic SQL `UPDATE` renderer.
#[derive(Debug, Clone, Copy, Default)]
pub struct SqlUpdate;

impl<D: Dialect> UpdateRenderer<D> for SqlUpdate {
    fn write(&self, plan: UpdatePlan<'_, D>, writer: &mut SqlWriter<'_, D>) -> Result<()> {
        writer.write_update();
        push_table(writer, plan.table.schema, plan.table.name);
        writer.write_set();

        for (index, (column, value)) in plan.assignments.iter_mut().enumerate() {
            if index > 0 {
                writer.write_comma();
            }
            writer.push_ident(column);
            writer.write_equals();
            value.render(writer)?;
        }

        write_filters(writer, plan.filters)?;

        if plan.returning {
            writer.write_returning();
        }

        Ok(())
    }
}

/// The generic SQL `DELETE` renderer.
#[derive(Debug, Clone, Copy, Default)]
pub struct SqlDelete;

impl<D: Dialect> DeleteRenderer<D> for SqlDelete {
    fn write(&self, plan: DeletePlan<'_, D>, writer: &mut SqlWriter<'_, D>) -> Result<()> {
        writer.write_delete();
        push_table(writer, plan.table.schema, plan.table.name);
        write_filters(writer, plan.filters)?;

        if plan.returning {
            writer.write_returning();
        }

        Ok(())
    }
}

/// The generic SQL `UPSERT` renderer.
///
/// Renders the widely supported `ON CONFLICT` form.
#[derive(Debug, Clone, Copy, Default)]
pub struct SqlUpsert;

impl<D: Dialect> UpsertRenderer<D> for SqlUpsert {
    fn write(&self, plan: UpsertPlan<'_, D>, writer: &mut SqlWriter<'_, D>) -> Result<()> {
        writer.write_insert();
        push_table(writer, plan.table.schema, plan.table.name);
        writer.write_group_start();
        for (index, column) in plan.columns.iter().enumerate() {
            if index > 0 {
                writer.write_comma();
            }
            writer.push_ident(column);
        }
        writer.write_group_end();
        writer.write_values();

        for (index, value) in plan.values.iter_mut().enumerate() {
            if index > 0 {
                writer.write_comma();
            }
            value.render(writer)?;
        }
        writer.write_group_end();

        writer.write_on_conflict();
        writer.write_group_start();
        for (index, column) in plan.conflict_columns.iter().enumerate() {
            if index > 0 {
                writer.write_comma();
            }
            writer.push_ident(column);
        }
        writer.write_group_end();
        writer.write_do_update_set();

        for (index, (column, value)) in plan.assignments.iter_mut().enumerate() {
            if index > 0 {
                writer.write_comma();
            }
            writer.push_ident(column);
            writer.write_equals();
            value.render(writer)?;
        }

        if plan.returning {
            writer.write_returning();
        }

        Ok(())
    }
}

pub(crate) fn write_filters<D: Dialect>(
    writer: &mut SqlWriter<'_, D>,
    filters: &mut [Expr<bool, D>],
) -> Result<()> {
    if filters.is_empty() {
        return Ok(());
    }

    writer.write_where();
    for (index, filter) in filters.iter_mut().enumerate() {
        if index > 0 {
            writer.write_and();
        }
        writer.write_group_start();
        filter.render(writer)?;
        writer.write_group_end();
    }

    Ok(())
}

fn write_from_filters<D: Dialect>(
    writer: &mut SqlWriter<'_, D>,
    table: &'static TableDef,
    filters: &mut [Expr<bool, D>],
    joins: &mut [Join<D>],
) -> Result<()> {
    writer.write_from();
    push_table(writer, table.schema, table.name);

    for join in joins {
        join.render(writer)?;
    }

    if !filters.is_empty() {
        writer.write_where();
        for (index, filter) in filters.iter_mut().enumerate() {
            if index > 0 {
                writer.write_and();
            }
            writer.write_group_start();
            filter.render(writer)?;
            writer.write_group_end();
        }
    }

    Ok(())
}

fn write_groups_having<D: Dialect>(
    writer: &mut SqlWriter<'_, D>,
    groups: &mut [Expr<(), D>],
    having: &mut [Expr<bool, D>],
) -> Result<()> {
    if !groups.is_empty() {
        writer.write_group_by();
        for (index, group) in groups.iter_mut().enumerate() {
            if index > 0 {
                writer.write_comma();
            }
            group.render(writer)?;
        }
    }

    if !having.is_empty() {
        writer.write_having();
        for (index, filter) in having.iter_mut().enumerate() {
            if index > 0 {
                writer.write_and();
            }
            writer.write_group_start();
            filter.render(writer)?;
            writer.write_group_end();
        }
    }

    Ok(())
}

fn write_orders<D: Dialect>(writer: &mut SqlWriter<'_, D>, orders: &mut [Order<D>]) -> Result<()> {
    if !orders.is_empty() {
        writer.write_order_by();
        for (index, order) in orders.iter_mut().enumerate() {
            if index > 0 {
                writer.write_comma();
            }
            order.render(writer)?;
        }
    }

    Ok(())
}

/// A scalar result used internally for single-column queries.
pub struct Scalar<T>(pub T);

impl<'r, T, R> FromRow<'r, R> for Scalar<T>
where
    R: sqlx_core::row::Row,
    usize: sqlx_core::column::ColumnIndex<R>,
    T: sqlx_core::decode::Decode<'r, R::Database> + sqlx_core::types::Type<R::Database>,
{
    fn from_row(row: &'r R) -> Result<Self> {
        Ok(Self(row.try_get(0)?))
    }
}

/// Converts a table query into a dialect-bound `Select`.
pub trait IntoSelect<D: Dialect + Clone> {
    type Table;

    fn into_select(self, db: &Db<D>) -> Select<D, Self::Table>;
}

impl<D, T> IntoSelect<D> for Query<T>
where
    D: Dialect + Clone,
    T: Table + 'static,
{
    type Table = T;

    fn into_select(self, db: &Db<D>) -> Select<D, T> {
        Select::new(db.clone())
    }
}

impl<D, T, K> IntoSelect<D> for Get<T, K>
where
    D: Dialect + Clone,
    T: Table + 'static,
    K: Clone + for<'q> Encode<'q, D::DB> + Type<D::DB> + Send + 'static,
{
    type Table = T;

    fn into_select(self, db: &Db<D>) -> Select<D, T> {
        let select = db.query(Query::<T>::new());

        match T::DEF.primary_key.first() {
            Some(primary_key) => select.filter(
                Column::<T>::new(T::DEF.schema, T::DEF.name, primary_key).eq::<D, _>(self.key),
            ),
            None => select.filter(Expr::<bool, D>::raw("1 = 0")),
        }
    }
}

/// A typed table column.
pub struct Column<T> {
    schema: Option<&'static str>,
    table: &'static str,
    alias: Option<&'static str>,
    column: &'static str,
    _marker: PhantomData<fn() -> T>,
}

impl<T, D: Dialect> IntoExpr<T, D> for Column<T>
where
    T: 'static,
{
    fn into_expr(self) -> Expr<T, D> {
        self.expr()
    }
}

impl<T> Column<T> {
    pub const fn new(
        schema: Option<&'static str>,
        table: &'static str,
        column: &'static str,
    ) -> Self {
        Self::with_alias(schema, table, None, column)
    }

    pub const fn with_alias(
        schema: Option<&'static str>,
        table: &'static str,
        alias: Option<&'static str>,
        column: &'static str,
    ) -> Self {
        Self {
            schema,
            table,
            alias,
            column,
            _marker: PhantomData,
        }
    }

    /// The SQL column name.
    pub const fn name(&self) -> &'static str {
        self.column
    }

    /// The SQL table name.
    pub const fn table(&self) -> &'static str {
        self.table
    }

    /// The SQL schema name, if the table is schema-qualified.
    pub const fn schema(&self) -> Option<&'static str> {
        self.schema
    }

    fn expr<D: Dialect>(&self) -> Expr<T, D>
    where
        T: 'static,
    {
        Expr::aliased_column(self.schema, self.table, self.alias, self.column)
    }

    pub fn eq<D, V>(self, value: V) -> Expr<bool, D>
    where
        D: Dialect,
        T: 'static,
        V: IntoExpr<T, D>,
    {
        self.expr::<D>().eq(value)
    }

    pub fn ne<D, V>(self, value: V) -> Expr<bool, D>
    where
        D: Dialect,
        T: 'static,
        V: IntoExpr<T, D>,
    {
        self.expr::<D>().ne(value)
    }

    pub fn gt<D, V>(self, value: V) -> Expr<bool, D>
    where
        D: Dialect,
        T: 'static,
        V: IntoExpr<T, D>,
    {
        self.expr::<D>().gt(value)
    }

    pub fn ge<D, V>(self, value: V) -> Expr<bool, D>
    where
        D: Dialect,
        T: 'static,
        V: IntoExpr<T, D>,
    {
        self.expr::<D>().ge(value)
    }

    pub fn lt<D, V>(self, value: V) -> Expr<bool, D>
    where
        D: Dialect,
        T: 'static,
        V: IntoExpr<T, D>,
    {
        self.expr::<D>().lt(value)
    }

    pub fn le<D, V>(self, value: V) -> Expr<bool, D>
    where
        D: Dialect,
        T: 'static,
        V: IntoExpr<T, D>,
    {
        self.expr::<D>().le(value)
    }

    pub fn like<D, V>(self, pattern: V) -> Expr<bool, D>
    where
        D: Dialect,
        T: 'static,
        V: IntoExpr<T, D>,
    {
        self.expr::<D>().like(pattern)
    }

    pub fn is_null<D: Dialect>(self) -> Expr<bool, D>
    where
        T: 'static,
    {
        self.expr::<D>().is_null()
    }

    pub fn is_not_null<D: Dialect>(self) -> Expr<bool, D>
    where
        T: 'static,
    {
        self.expr::<D>().is_not_null()
    }

    pub fn in_list<D, V, I>(self, values: I) -> Expr<bool, D>
    where
        D: Dialect,
        T: 'static,
        V: IntoExpr<T, D>,
        I: IntoIterator<Item = V>,
    {
        self.expr::<D>().in_list(values)
    }

    pub fn not_in_list<D, V, I>(self, values: I) -> Expr<bool, D>
    where
        D: Dialect,
        T: 'static,
        V: IntoExpr<T, D>,
        I: IntoIterator<Item = V>,
    {
        self.expr::<D>().not_in_list(values)
    }

    pub(crate) fn render_column<D: Dialect>(&self, writer: &mut SqlWriter<'_, D>) {
        if let Some(alias) = self.alias {
            writer.push_ident(alias);
        } else {
            if let Some(schema) = self.schema {
                writer.push_ident(schema);
                writer.write_identifier_separator();
            }
            writer.push_ident(self.table);
        }
        writer.write_identifier_separator();
        writer.push_ident(self.column);
    }

    pub fn in_query<D, S>(self, subquery: S) -> Expr<bool, D>
    where
        D: Dialect,
        T: 'static,
        S: Subquery<D> + 'static,
    {
        Expr::<T, D>::in_subquery(self.expr::<D>(), subquery, false)
    }

    pub fn not_in_query<D, S>(self, subquery: S) -> Expr<bool, D>
    where
        D: Dialect,
        T: 'static,
        S: Subquery<D> + 'static,
    {
        Expr::<T, D>::in_subquery(self.expr::<D>(), subquery, true)
    }

    pub fn count<D: Dialect>(self) -> Expr<i64, D>
    where
        T: 'static,
    {
        self.expr::<D>().count()
    }

    pub fn count_distinct<D: Dialect>(self) -> Expr<i64, D>
    where
        T: 'static,
    {
        self.expr::<D>().count_distinct()
    }

    pub fn asc<D: Dialect>(self) -> Order<D>
    where
        T: 'static,
    {
        self.expr::<D>().into_order(false)
    }

    pub fn desc<D: Dialect>(self) -> Order<D>
    where
        T: 'static,
    {
        self.expr::<D>().into_order(true)
    }
}

/// A typed query bound to a dialect and database handle.
pub struct Select<D: Dialect + Clone, T: 'static> {
    db: Db<D>,
    distinct: bool,
    filters: Vec<Expr<bool, D>>,
    joins: Vec<Join<D>>,
    orders: Vec<Order<D>>,
    includes: Vec<Box<dyn Include<D, T>>>,
    ctes: Vec<Cte<D>>,
    limit: Option<u64>,
    offset: Option<u64>,
}

impl<D: Dialect + Clone, T: Table + 'static> Select<D, T> {
    fn new(db: Db<D>) -> Self {
        Self {
            db,
            distinct: false,
            filters: Vec::new(),
            joins: Vec::new(),
            orders: Vec::new(),
            includes: Vec::new(),
            ctes: Vec::new(),
            limit: None,
            offset: None,
        }
    }

    pub fn filter(mut self, filter: Expr<bool, D>) -> Self {
        self.filters.push(filter);
        self
    }

    pub fn distinct(mut self) -> Self {
        self.distinct = true;
        self
    }

    /// Adds a common table expression before the query.
    pub fn with(mut self, cte: Cte<D>) -> Self {
        self.ctes.push(cte);
        self
    }

    pub fn join(mut self, join: Join<D>) -> Self {
        self.joins.push(join);
        self
    }

    pub fn order_by(mut self, order: Order<D>) -> Self {
        self.orders.push(order);
        self
    }

    pub fn limit(mut self, limit: u64) -> Self {
        self.limit = Some(limit);
        self
    }

    pub fn offset(mut self, offset: u64) -> Self {
        self.offset = Some(offset);
        self
    }

    pub fn select<P>(self, projection: P) -> ProjectedSelect<D, T, P>
    where
        P: Projection<D>,
    {
        ProjectedSelect {
            db: self.db,
            distinct: self.distinct,
            filters: self.filters,
            joins: self.joins,
            orders: self.orders,
            limit: self.limit,
            offset: self.offset,
            projection,
            groups: Vec::new(),
            having: Vec::new(),
            ctes: self.ctes,
            _source: PhantomData,
        }
    }

    pub fn include<I>(mut self, include: I) -> Self
    where
        I: Include<D, T> + 'static,
    {
        self.includes.push(Box::new(include));
        self
    }

    pub async fn exec(mut self) -> Result<Vec<T>>
    where
        T: Table + Send + Unpin + for<'r> FromRow<'r, <D::DB as Database>::Row>,
        <D::DB as Database>::Arguments: sqlx_core::arguments::IntoArguments<D::DB>,
        for<'c> &'c mut <D::DB as Database>::Connection: Executor<'c, Database = D::DB>,
    {
        let mut writer = SqlWriter::new(self.db.dialect());
        write_ctes(&mut writer, &mut self.ctes)?;
        let mut projection = StarProjection;

        D::Select::default().write(
            SelectPlan {
                table: T::DEF,
                distinct: self.distinct,
                projection: &mut projection,
                filters: &mut self.filters,
                joins: &mut self.joins,
                groups: &mut [],
                having: &mut [],
                orders: &mut self.orders,
                limit: self.limit,
                offset: self.offset,
            },
            &mut writer,
        )?;

        let (sql, arguments) = writer.finish();
        let mut rows: Vec<T> =
            query_as_with::<D::DB, T, _>(AssertSqlSafe(sql).into_sql_str(), arguments)
                .fetch_all(self.db.pool())
                .await?;

        for include in &self.includes {
            include.apply(&self.db, &mut rows).await?;
        }

        Ok(rows)
    }

    pub async fn exec_in(mut self, tx: &mut crate::db::Transaction<'_, D>) -> Result<Vec<T>>
    where
        T: Table + Send + Unpin + for<'r> FromRow<'r, <D::DB as Database>::Row>,
        <D::DB as Database>::Arguments: sqlx_core::arguments::IntoArguments<D::DB>,
        for<'c> &'c mut <D::DB as Database>::Connection: Executor<'c, Database = D::DB>,
    {
        let mut writer = SqlWriter::new(self.db.dialect());
        write_ctes(&mut writer, &mut self.ctes)?;
        let mut projection = StarProjection;

        D::Select::default().write(
            SelectPlan {
                table: T::DEF,
                distinct: self.distinct,
                projection: &mut projection,
                filters: &mut self.filters,
                joins: &mut self.joins,
                groups: &mut [],
                having: &mut [],
                orders: &mut self.orders,
                limit: self.limit,
                offset: self.offset,
            },
            &mut writer,
        )?;

        let (sql, arguments) = writer.finish();
        let mut rows: Vec<T> =
            query_as_with::<D::DB, T, _>(AssertSqlSafe(sql).into_sql_str(), arguments)
                .fetch_all(tx.connection())
                .await?;

        for include in &self.includes {
            include.apply_in(&self.db, tx, &mut rows).await?;
        }

        Ok(rows)
    }

    pub async fn count(mut self) -> Result<u64>
    where
        T: Table,
        usize: sqlx_core::column::ColumnIndex<<D::DB as Database>::Row>,
        i64: for<'q> sqlx_core::decode::Decode<'q, D::DB> + sqlx_core::types::Type<D::DB>,
        <D::DB as Database>::Arguments: sqlx_core::arguments::IntoArguments<D::DB>,
        for<'c> &'c mut <D::DB as Database>::Connection: Executor<'c, Database = D::DB>,
    {
        let mut writer = SqlWriter::new(self.db.dialect());
        write_ctes(&mut writer, &mut self.ctes)?;
        D::Select::default().write_count_star(
            CountPlan {
                table: T::DEF,
                distinct: self.distinct,
                filters: &mut self.filters,
                joins: &mut self.joins,
            },
            &mut writer,
        )?;

        let (sql, arguments) = writer.finish();
        let (count,): (i64,) =
            query_as_with::<D::DB, (i64,), _>(AssertSqlSafe(sql).into_sql_str(), arguments)
                .fetch_one(self.db.pool())
                .await?;

        Ok(count as u64)
    }

    pub async fn first(mut self) -> Result<Option<T>>
    where
        T: Table + Send + Unpin + for<'r> FromRow<'r, <D::DB as Database>::Row>,
        <D::DB as Database>::Arguments: sqlx_core::arguments::IntoArguments<D::DB>,
        for<'c> &'c mut <D::DB as Database>::Connection: Executor<'c, Database = D::DB>,
    {
        self.limit = Some(1);
        let mut rows = self.exec().await?;
        Ok(rows.pop())
    }
}
