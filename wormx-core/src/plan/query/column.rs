use std::marker::PhantomData;

use super::cte::Subquery;
use crate::plan::backend::Dialect;
use crate::plan::expr::{ColumnRef, Expr, IntoExpr, Order};

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
