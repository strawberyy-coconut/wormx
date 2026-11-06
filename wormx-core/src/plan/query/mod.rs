//! Typed query builders, projections, and operation renderers.

mod plan;

pub use plan::*;

use std::marker::PhantomData;

use sqlx_core::database::Database;
use sqlx_core::encode::Encode;
use sqlx_core::error::{Error, Result};
use sqlx_core::executor::Executor;
use sqlx_core::from_row::FromRow;
use sqlx_core::query_as::query_as_with;
use sqlx_core::sql_str::{AssertSqlSafe, SqlSafeStr};
use sqlx_core::types::Type;

use super::backend::Dialect;
use super::command::Writer;
use super::expr::{ColumnRef, Expr, ExprKind, IntoExpr, Order};
use super::include::Include;
use super::join::{Join, TableSource};
use crate::exec::Db;
use crate::schema::Table;

/// A dialect-neutral table query.
pub struct Query<T> {
    _marker: PhantomData<fn() -> T>,
}

impl<T> Query<T> {
    /// Creates an empty table query.
    pub const fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }

    /// Adds a filter to the table query.
    pub fn filter<D>(self, filter: Expr<bool, D>) -> Filtered<D, T>
    where
        D: Dialect + Clone,
        T: Table + 'static,
    {
        Filtered::new(filter)
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
    /// Creates a filtered table query.
    pub(crate) fn new(filter: Expr<bool, D>) -> Self {
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

/// A visitor for the items in a projection.
///
/// The dialect implements this to render a projection in its own language.
pub trait ProjectionVisitor<D: Dialect> {
    fn star(&mut self) -> Result<()>;
    fn column(&mut self, column: &ColumnRef) -> Result<()>;
    fn expr(&mut self, expr: &mut ExprKind<D>) -> Result<()>;
    fn separator(&mut self) -> Result<()>;
    fn begin_count(&mut self, distinct: bool) -> Result<()>;
    fn end_count(&mut self) -> Result<()>;
}

/// The `*` projection used by generated table selects.
#[derive(Debug, Clone, Copy, Default)]
pub struct StarProjection;

impl<D: Dialect> Projection<D> for StarProjection {
    type Output = ();
    type Row = ();

    fn visit(&mut self, out: &mut dyn ProjectionVisitor<D>) -> Result<()> {
        out.star()
    }

    fn map(row: Self::Row) -> Self::Output {
        row
    }
}

/// A selectable projection: one column/expression, or a tuple of up to 16.
///
/// The tuple limit matches SQLx's `FromRow` support, so every projection can
/// be decoded.
pub trait Projection<D: Dialect>: Send {
    type Output;
    type Row: Send + Unpin;

    fn visit(&mut self, out: &mut dyn ProjectionVisitor<D>) -> Result<()>;

    fn map(row: Self::Row) -> Self::Output;
}

/// Marker for projections that can be wrapped in `COUNT(...)`.
pub trait CountProjection<D: Dialect>: Projection<D> {}

impl<D: Dialect, C: Send + Unpin> Projection<D> for Column<C> {
    type Output = C;
    type Row = Scalar<C>;

    fn visit(&mut self, out: &mut dyn ProjectionVisitor<D>) -> Result<()> {
        out.column(&self.column_ref())
    }

    fn map(row: Self::Row) -> Self::Output {
        row.0
    }
}

impl<D: Dialect, C: Send + Unpin> CountProjection<D> for Column<C> {}

impl<D: Dialect, C: Send + Unpin> Projection<D> for Expr<C, D> {
    type Output = C;
    type Row = Scalar<C>;

    fn visit(&mut self, out: &mut dyn ProjectionVisitor<D>) -> Result<()> {
        out.expr(self.kind_mut())
    }

    fn map(row: Self::Row) -> Self::Output {
        row.0
    }
}

impl<D: Dialect, C: Send + Unpin> CountProjection<D> for Expr<C, D> {}

// The macro receives each tuple index because a macro_rules! repetition cannot
// generate positional field access on its own.
macro_rules! impl_projection_tuple {
    ($($ty:ident : $index:tt),+ $(,)?) => {
        impl<D, $($ty),+> Projection<D> for ($($ty,)+)
        where
            D: Dialect,
            $($ty: Projection<D>,)+
            $($ty::Output: Send + Unpin,)+
        {
            type Output = ($($ty::Output,)+);
            type Row = ($($ty::Output,)+);

            fn visit(&mut self, out: &mut dyn ProjectionVisitor<D>) -> Result<()> {
                let mut first = true;
                $(
                    if !first {
                        out.separator()?;
                    }
                    first = false;
                    self.$index.visit(out)?;
                )+
                Ok(())
            }

            fn map(row: Self::Row) -> Self::Output {
                row
            }
        }
    };
}

impl_projection_tuple!(A: 0, B: 1);
impl_projection_tuple!(A: 0, B: 1, C: 2);
impl_projection_tuple!(A: 0, B: 1, C: 2, E: 3);
impl_projection_tuple!(A: 0, B: 1, C: 2, E: 3, F: 4);
impl_projection_tuple!(A: 0, B: 1, C: 2, E: 3, F: 4, G: 5);
impl_projection_tuple!(A: 0, B: 1, C: 2, E: 3, F: 4, G: 5, H: 6);
impl_projection_tuple!(A: 0, B: 1, C: 2, E: 3, F: 4, G: 5, H: 6, I: 7);
impl_projection_tuple!(A: 0, B: 1, C: 2, E: 3, F: 4, G: 5, H: 6, I: 7, J: 8);
impl_projection_tuple!(A: 0, B: 1, C: 2, E: 3, F: 4, G: 5, H: 6, I: 7, J: 8, K: 9);
impl_projection_tuple!(A: 0, B: 1, C: 2, E: 3, F: 4, G: 5, H: 6, I: 7, J: 8, K: 9, L: 10);
impl_projection_tuple!(A: 0, B: 1, C: 2, E: 3, F: 4, G: 5, H: 6, I: 7, J: 8, K: 9, L: 10, M: 11);
impl_projection_tuple!(A: 0, B: 1, C: 2, E: 3, F: 4, G: 5, H: 6, I: 7, J: 8, K: 9, L: 10, M: 11, N: 12);
impl_projection_tuple!(A: 0, B: 1, C: 2, E: 3, F: 4, G: 5, H: 6, I: 7, J: 8, K: 9, L: 10, M: 11, N: 12, O: 13);
impl_projection_tuple!(A: 0, B: 1, C: 2, E: 3, F: 4, G: 5, H: 6, I: 7, J: 8, K: 9, L: 10, M: 11, N: 12, O: 13, P: 14);
impl_projection_tuple!(A: 0, B: 1, C: 2, E: 3, F: 4, G: 5, H: 6, I: 7, J: 8, K: 9, L: 10, M: 11, N: 12, O: 13, P: 14, Q: 15);

/// Shared builder state for row and projected selects.
struct SelectState<D: Dialect + Clone, T: 'static> {
    db: Db<D>,
    source: TableSource,
    distinct: bool,
    filters: Vec<Expr<bool, D>>,
    joins: Vec<Join<D>>,
    orders: Vec<Order<D>>,
    ctes: Vec<Cte<D>>,
    limit: Option<u64>,
    offset: Option<u64>,
    tail: Option<&'static str>,
    _marker: PhantomData<fn() -> T>,
}

impl<D: Dialect + Clone, T: Table + 'static> SelectState<D, T> {
    fn new(db: Db<D>) -> Self {
        Self {
            db,
            source: TableSource::new(T::DEF, None),
            distinct: false,
            filters: Vec::new(),
            joins: Vec::new(),
            orders: Vec::new(),
            ctes: Vec::new(),
            limit: None,
            offset: None,
            tail: None,
            _marker: PhantomData,
        }
    }

    fn filter(mut self, filter: Expr<bool, D>) -> Self {
        self.filters.push(filter);
        self
    }

    fn distinct(mut self) -> Self {
        self.distinct = true;
        self
    }

    fn with(mut self, cte: Cte<D>) -> Self {
        self.ctes.push(cte);
        self
    }

    fn join(mut self, join: Join<D>) -> Self {
        self.joins.push(join);
        self
    }

    fn order_by(mut self, order: Order<D>) -> Self {
        self.orders.push(order);
        self
    }

    fn limit(mut self, limit: u64) -> Self {
        self.limit = Some(limit);
        self
    }

    fn offset(mut self, offset: u64) -> Self {
        self.offset = Some(offset);
        self
    }

    fn with_source_suffix(mut self, suffix: &'static str) -> Self {
        self.source.suffix = Some(suffix);
        self
    }

    fn with_tail(mut self, tail: &'static str) -> Self {
        self.tail = Some(tail);
        self
    }

    fn write<P: Projection<D>>(
        &mut self,
        projection: &mut P,
        groups: &mut [Expr<(), D>],
        having: &mut [Expr<bool, D>],
        writer: &mut Writer<D>,
    ) -> Result<()> {
        D::Select::default().write(
            SelectPlan {
                table: self.source,
                distinct: self.distinct,
                projection,
                ctes: &mut self.ctes,
                filters: &mut self.filters,
                joins: &mut self.joins,
                groups,
                having,
                orders: &mut self.orders,
                limit: self.limit,
                offset: self.offset,
                tail: self.tail,
            },
            writer,
        )
    }

    fn write_count<P>(
        &mut self,
        projection: &mut P,
        groups: &mut [Expr<(), D>],
        having: &mut [Expr<bool, D>],
        writer: &mut Writer<D>,
        distinct: bool,
    ) -> Result<()>
    where
        P: Projection<D> + CountProjection<D>,
    {
        D::Select::default().write_count(
            SelectPlan {
                table: self.source,
                distinct: self.distinct,
                projection,
                ctes: &mut self.ctes,
                filters: &mut self.filters,
                joins: &mut self.joins,
                groups,
                having,
                orders: &mut self.orders,
                limit: self.limit,
                offset: self.offset,
                tail: self.tail,
            },
            writer,
            distinct,
        )
    }

    fn write_count_star(&mut self, writer: &mut Writer<D>) -> Result<()> {
        D::Select::default().write_count_star(
            CountPlan {
                table: self.source,
                distinct: self.distinct,
                filters: &mut self.filters,
                joins: &mut self.joins,
                ctes: &mut self.ctes,
                tail: self.tail,
            },
            writer,
        )
    }
}

/// A projected table query bound to a dialect and database handle.
pub struct ProjectedSelect<D: Dialect + Clone, T: Table + 'static, P: Projection<D>> {
    state: SelectState<D, T>,
    projection: P,
    groups: Vec<Expr<(), D>>,
    having: Vec<Expr<bool, D>>,
    dropped_includes: bool,
}

/// Single-column shorthand.
pub type ColumnSelect<D, T, C> = ProjectedSelect<D, T, Column<C>>;

/// A subquery that can be rendered inside an expression.
pub trait Subquery<D: Dialect>: Send {
    fn render(&mut self, writer: &mut Writer<D>) -> Result<()>;
}

/// A common table expression: `name AS (query)`.
pub struct Cte<D: Dialect> {
    name: String,
    query: Box<dyn Subquery<D>>,
}

impl<D: Dialect> Cte<D> {
    /// Creates a common table expression.
    pub fn new(name: impl Into<String>, query: impl Subquery<D> + 'static) -> Self {
        Self {
            name: name.into(),
            query: Box::new(query),
        }
    }

    /// Writes the CTE into the query.
    pub fn render(&mut self, writer: &mut Writer<D>) -> Result<()> {
        writer.push_ident(&self.name);
        writer.push(" AS (");
        self.query.render(writer)?;
        writer.push(")");
        Ok(())
    }
}

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
    pub fn with_source_suffix(mut self, suffix: &'static str) -> Self {
        self.state = self.state.with_source_suffix(suffix);
        self
    }

    /// Attaches an opaque, dialect-defined tail to the statement.
    pub fn with_tail(mut self, tail: &'static str) -> Self {
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

    /// Compares the column with `value` using `=`.
    pub fn eq<D, V>(self, value: V) -> Expr<bool, D>
    where
        D: Dialect,
        T: 'static,
        V: IntoExpr<T, D>,
    {
        self.expr::<D>().eq(value)
    }

    /// Compares the column with `value` using `<>`.
    pub fn ne<D, V>(self, value: V) -> Expr<bool, D>
    where
        D: Dialect,
        T: 'static,
        V: IntoExpr<T, D>,
    {
        self.expr::<D>().ne(value)
    }

    /// Compares the column with `value` using `>`.
    pub fn gt<D, V>(self, value: V) -> Expr<bool, D>
    where
        D: Dialect,
        T: 'static,
        V: IntoExpr<T, D>,
    {
        self.expr::<D>().gt(value)
    }

    /// Compares the column with `value` using `>=`.
    pub fn ge<D, V>(self, value: V) -> Expr<bool, D>
    where
        D: Dialect,
        T: 'static,
        V: IntoExpr<T, D>,
    {
        self.expr::<D>().ge(value)
    }

    /// Compares the column with `value` using `<`.
    pub fn lt<D, V>(self, value: V) -> Expr<bool, D>
    where
        D: Dialect,
        T: 'static,
        V: IntoExpr<T, D>,
    {
        self.expr::<D>().lt(value)
    }

    /// Compares the column with `value` using `<=`.
    pub fn le<D, V>(self, value: V) -> Expr<bool, D>
    where
        D: Dialect,
        T: 'static,
        V: IntoExpr<T, D>,
    {
        self.expr::<D>().le(value)
    }

    /// Matches the column against a `LIKE` pattern.
    pub fn like<D, V>(self, pattern: V) -> Expr<bool, D>
    where
        D: Dialect,
        T: 'static,
        V: IntoExpr<T, D>,
    {
        self.expr::<D>().like(pattern)
    }

    /// Tests whether the column is `NULL`.
    pub fn is_null<D: Dialect>(self) -> Expr<bool, D>
    where
        T: 'static,
    {
        self.expr::<D>().is_null()
    }

    /// Tests whether the column is not `NULL`.
    pub fn is_not_null<D: Dialect>(self) -> Expr<bool, D>
    where
        T: 'static,
    {
        self.expr::<D>().is_not_null()
    }

    /// Tests whether the column is in a list of values.
    pub fn in_list<D, V, I>(self, values: I) -> Expr<bool, D>
    where
        D: Dialect,
        T: 'static,
        V: IntoExpr<T, D>,
        I: IntoIterator<Item = V>,
    {
        self.expr::<D>().in_list(values)
    }

    /// Tests whether the column is not in a list of values.
    pub fn not_in_list<D, V, I>(self, values: I) -> Expr<bool, D>
    where
        D: Dialect,
        T: 'static,
        V: IntoExpr<T, D>,
        I: IntoIterator<Item = V>,
    {
        self.expr::<D>().not_in_list(values)
    }

    pub(crate) fn column_ref(&self) -> ColumnRef {
        ColumnRef {
            schema: self.schema.map(str::to_owned),
            table: self.table.to_owned(),
            alias: self.alias.map(str::to_owned),
            column: self.column.to_owned(),
        }
    }

    /// Tests whether the column is in a subquery result.
    pub fn in_query<D, S>(self, subquery: S) -> Expr<bool, D>
    where
        D: Dialect,
        T: 'static,
        S: Subquery<D> + 'static,
    {
        Expr::<T, D>::in_subquery(self.expr::<D>(), subquery, false)
    }

    /// Tests whether the column is not in a subquery result.
    pub fn not_in_query<D, S>(self, subquery: S) -> Expr<bool, D>
    where
        D: Dialect,
        T: 'static,
        S: Subquery<D> + 'static,
    {
        Expr::<T, D>::in_subquery(self.expr::<D>(), subquery, true)
    }

    /// Counts the column values.
    pub fn count<D: Dialect>(self) -> Expr<i64, D>
    where
        T: 'static,
    {
        self.expr::<D>().count()
    }

    /// Counts distinct column values.
    pub fn count_distinct<D: Dialect>(self) -> Expr<i64, D>
    where
        T: 'static,
    {
        self.expr::<D>().count_distinct()
    }

    /// Casts this column to the SQL type named by `to`.
    ///
    /// `to` is written into the query verbatim, so it must be a constant such
    /// as `"NVARCHAR(200)"` or `"DECIMAL(10, 2)"`. Never pass user input.
    pub fn cast<D: Dialect>(self, to: &'static str) -> Expr<T, D>
    where
        T: 'static,
    {
        self.expr::<D>().cast(to)
    }

    /// Orders by the column ascending.
    pub fn asc<D: Dialect>(self) -> Order<D>
    where
        T: 'static,
    {
        self.expr::<D>().into_order(false)
    }

    /// Orders by the column descending.
    pub fn desc<D: Dialect>(self) -> Order<D>
    where
        T: 'static,
    {
        self.expr::<D>().into_order(true)
    }
}

/// A typed query bound to a dialect and database handle.
pub struct Select<D: Dialect + Clone, T: 'static> {
    state: SelectState<D, T>,
    includes: Vec<Box<dyn Include<D, T>>>,
}

impl<D: Dialect + Clone, T: Table + 'static> Select<D, T> {
    fn new(db: Db<D>) -> Self {
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
    pub fn with_source_suffix(mut self, suffix: &'static str) -> Self {
        self.state = self.state.with_source_suffix(suffix);
        self
    }

    /// Attaches an opaque, dialect-defined tail to the statement.
    pub fn with_tail(mut self, tail: &'static str) -> Self {
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
