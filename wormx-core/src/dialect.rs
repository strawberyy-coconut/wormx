use std::marker::PhantomData;

use sqlx_core::arguments::Arguments;
use sqlx_core::database::Database;
use sqlx_core::encode::Encode;
use sqlx_core::error::Error;
use sqlx_core::types::Type;

use crate::query::{
    DeleteRenderer, InsertRenderer, SelectRenderer, UpdateRenderer, UpsertRenderer,
};

/// Optional SQL features a dialect can provide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    pub select: bool,
    pub joins: bool,
    pub group_by: bool,
    pub aggregates: bool,
    pub subqueries: bool,
    pub scan: bool,
    pub returning: bool,
    pub full_text_search: bool,
    pub json: bool,
    pub arrays: bool,
    pub upsert: bool,
    pub generated_columns: bool,
}

impl Capabilities {
    pub const NONE: Self = Self {
        select: false,
        joins: false,
        group_by: false,
        aggregates: false,
        subqueries: false,
        scan: false,
        returning: false,
        full_text_search: false,
        json: false,
        arrays: false,
        upsert: false,
        generated_columns: false,
    };

    pub const ALL: Self = Self {
        select: true,
        joins: true,
        group_by: true,
        aggregates: true,
        subqueries: true,
        scan: true,
        returning: true,
        full_text_search: true,
        json: true,
        arrays: true,
        upsert: true,
        generated_columns: true,
    };
}

/// A backend dialect.
///
/// The dialect owns the capability set, chooses the renderer for each logical
/// operation, and exposes the SQL syntax hooks used by the generic renderers.
/// A SQL driver overrides only the parts that differ, while a non-SQL dialect
/// can ignore the SQL hooks.
pub trait Dialect: 'static + Sized + Send + Sync + Clone {
    type DB: Database;

    /// The renderer used for `SELECT` operations.
    type Select: SelectRenderer<Self>;
    /// The renderer used for `INSERT` operations.
    type Insert: InsertRenderer<Self>;
    /// The renderer used for `UPDATE` operations.
    type Update: UpdateRenderer<Self>;
    /// The renderer used for `DELETE` operations.
    type Delete: DeleteRenderer<Self>;
    /// The renderer used for `UPSERT` operations.
    type Upsert: UpsertRenderer<Self>;

    const CAPABILITIES: Capabilities;

    fn quote_ident(&self, name: &str) -> String;

    fn placeholder(&self, index: usize) -> String {
        format!("?{index}")
    }

    /// Appends the dialect's LIMIT/OFFSET syntax.
    fn write_limit_offset(
        &self,
        _has_order: bool,
        limit: Option<u64>,
        offset: Option<u64>,
        out: &mut String,
    ) {
        if let Some(limit) = limit {
            out.push_str(&format!(" LIMIT {limit}"));
        }
        if let Some(offset) = offset {
            out.push_str(&format!(" OFFSET {offset}"));
        }
    }

    fn write_select(distinct: bool, writer: &mut SqlWriter<Self>) {
        writer.push("SELECT ");
        if distinct {
            writer.push("DISTINCT ");
        }
    }

    fn write_from(writer: &mut SqlWriter<Self>) {
        writer.push(" FROM ");
    }

    fn write_insert(writer: &mut SqlWriter<Self>) {
        writer.push("INSERT INTO ");
    }

    fn write_insert_default(writer: &mut SqlWriter<Self>) {
        writer.push(" DEFAULT VALUES");
    }

    fn write_values(writer: &mut SqlWriter<Self>) {
        writer.push(" VALUES (");
    }

    fn write_update(writer: &mut SqlWriter<Self>) {
        writer.push("UPDATE ");
    }

    fn write_set(writer: &mut SqlWriter<Self>) {
        writer.push(" SET ");
    }

    fn write_equals(writer: &mut SqlWriter<Self>) {
        writer.push(" = ");
    }

    fn write_delete(writer: &mut SqlWriter<Self>) {
        writer.push("DELETE FROM ");
    }

    fn write_returning(writer: &mut SqlWriter<Self>) {
        writer.push(" RETURNING ");
        writer.write_star();
    }

    fn write_on_conflict(writer: &mut SqlWriter<Self>) {
        writer.push(" ON CONFLICT ");
    }

    fn write_do_update_set(writer: &mut SqlWriter<Self>) {
        writer.push(" DO UPDATE SET ");
    }

    fn write_where(writer: &mut SqlWriter<Self>) {
        writer.push(" WHERE ");
    }

    fn write_and(writer: &mut SqlWriter<Self>) {
        writer.push(" AND ");
    }

    fn write_or(writer: &mut SqlWriter<Self>) {
        writer.push(" OR ");
    }

    fn write_join(kind: crate::join::JoinKind, writer: &mut SqlWriter<Self>) {
        writer.push(match kind {
            crate::join::JoinKind::Inner => " JOIN ",
            crate::join::JoinKind::Left => " LEFT JOIN ",
            crate::join::JoinKind::Right => " RIGHT JOIN ",
            crate::join::JoinKind::Full => " FULL JOIN ",
        });
    }

    fn write_on(writer: &mut SqlWriter<Self>) {
        writer.push(" ON ");
    }

    fn write_group_by(writer: &mut SqlWriter<Self>) {
        writer.push(" GROUP BY ");
    }

    fn write_partition_by(writer: &mut SqlWriter<Self>) {
        writer.push("PARTITION BY ");
    }

    fn write_over(writer: &mut SqlWriter<Self>) {
        writer.push(" OVER (");
    }

    fn write_having(writer: &mut SqlWriter<Self>) {
        writer.push(" HAVING ");
    }

    fn write_order_by(writer: &mut SqlWriter<Self>) {
        writer.push(" ORDER BY ");
    }

    fn write_asc(writer: &mut SqlWriter<Self>) {
        writer.push(" ASC");
    }

    fn write_desc(writer: &mut SqlWriter<Self>) {
        writer.push(" DESC");
    }

    fn write_as(writer: &mut SqlWriter<Self>) {
        writer.push(" AS ");
    }

    fn write_comma(writer: &mut SqlWriter<Self>) {
        writer.push(", ");
    }

    fn write_star(writer: &mut SqlWriter<Self>) {
        writer.push("*");
    }

    fn write_count_star(writer: &mut SqlWriter<Self>) {
        writer.push("COUNT(*)");
    }

    fn write_count_start(distinct: bool, writer: &mut SqlWriter<Self>) {
        writer.push("COUNT(");
        if distinct {
            writer.push("DISTINCT ");
        }
    }

    fn write_function_start(name: &str, writer: &mut SqlWriter<Self>) {
        writer.push(name);
        writer.write_group_start();
    }

    fn write_operator(operator: &str, writer: &mut SqlWriter<Self>) {
        writer.write_space();
        writer.push(operator);
        writer.write_space();
    }

    fn write_not(writer: &mut SqlWriter<Self>) {
        writer.push("NOT (");
    }

    fn write_in(negated: bool, writer: &mut SqlWriter<Self>) {
        writer.push(if negated { " NOT IN (" } else { " IN (" });
    }

    fn write_is_null(negated: bool, writer: &mut SqlWriter<Self>) {
        writer.push(if negated { " IS NOT NULL" } else { " IS NULL" });
    }

    fn write_like(writer: &mut SqlWriter<Self>) {
        writer.push(" LIKE ");
    }

    fn write_boolean_literal(value: bool, writer: &mut SqlWriter<Self>) {
        writer.push(if value { "1 = 1" } else { "1 = 0" });
    }

    fn write_group_start(writer: &mut SqlWriter<Self>) {
        writer.push("(");
    }

    fn write_group_end(writer: &mut SqlWriter<Self>) {
        writer.push(")");
    }

    fn write_identifier_separator(writer: &mut SqlWriter<Self>) {
        writer.push(".");
    }

    fn write_space(writer: &mut SqlWriter<Self>) {
        writer.push(" ");
    }
}

pub trait SupportsReturning: Dialect {}
pub trait SupportsFullTextSearch: Dialect {}
pub trait SupportsJson: Dialect {}
pub trait SupportsArrays: Dialect {}
pub trait SupportsUpsert: Dialect {}

/// Rust types that can be treated as textual SQL expressions.
pub trait Textual {}
impl Textual for String {}
impl Textual for str {}
impl Textual for &str {}
impl Textual for std::borrow::Cow<'_, str> {}

/// A writer for SQL text plus bound arguments.
pub struct SqlWriter<'a, D: Dialect> {
    sql: String,
    arguments: <D::DB as Database>::Arguments,
    dialect: &'a D,
}

impl<'a, D: Dialect> SqlWriter<'a, D> {
    pub fn new(dialect: &'a D) -> Self {
        Self {
            sql: String::new(),
            arguments: <D::DB as Database>::Arguments::default(),
            dialect,
        }
    }

    pub fn push(&mut self, sql: &str) {
        self.sql.push_str(sql);
    }

    pub fn push_ident(&mut self, ident: &str) {
        let quoted = self.dialect.quote_ident(ident);
        self.sql.push_str(&quoted);
    }

    pub fn push_limit_offset(&mut self, has_order: bool, limit: Option<u64>, offset: Option<u64>) {
        let mut suffix = String::new();
        self.dialect
            .write_limit_offset(has_order, limit, offset, &mut suffix);
        self.sql.push_str(&suffix);
    }

    pub fn write_select(&mut self, distinct: bool) {
        D::write_select(distinct, self);
    }

    pub fn write_from(&mut self) {
        D::write_from(self);
    }

    pub fn write_where(&mut self) {
        D::write_where(self);
    }

    pub fn write_insert(&mut self) {
        D::write_insert(self);
    }

    pub fn write_insert_default(&mut self) {
        D::write_insert_default(self);
    }

    pub fn write_values(&mut self) {
        D::write_values(self);
    }

    pub fn write_update(&mut self) {
        D::write_update(self);
    }

    pub fn write_set(&mut self) {
        D::write_set(self);
    }

    pub fn write_equals(&mut self) {
        D::write_equals(self);
    }

    pub fn write_delete(&mut self) {
        D::write_delete(self);
    }

    pub fn write_returning(&mut self) {
        D::write_returning(self);
    }

    pub fn write_on_conflict(&mut self) {
        D::write_on_conflict(self);
    }

    pub fn write_do_update_set(&mut self) {
        D::write_do_update_set(self);
    }

    pub fn write_and(&mut self) {
        D::write_and(self);
    }

    pub fn write_or(&mut self) {
        D::write_or(self);
    }

    pub fn write_join(&mut self, kind: crate::join::JoinKind) {
        D::write_join(kind, self);
    }

    pub fn write_on(&mut self) {
        D::write_on(self);
    }

    pub fn write_group_by(&mut self) {
        D::write_group_by(self);
    }

    pub fn write_partition_by(&mut self) {
        D::write_partition_by(self);
    }

    pub fn write_over(&mut self) {
        D::write_over(self);
    }

    pub fn write_having(&mut self) {
        D::write_having(self);
    }

    pub fn write_order_by(&mut self) {
        D::write_order_by(self);
    }

    pub fn write_asc(&mut self) {
        D::write_asc(self);
    }

    pub fn write_desc(&mut self) {
        D::write_desc(self);
    }

    pub fn write_as(&mut self) {
        D::write_as(self);
    }

    pub fn write_comma(&mut self) {
        D::write_comma(self);
    }

    pub fn write_star(&mut self) {
        D::write_star(self);
    }

    pub fn write_count_star(&mut self) {
        D::write_count_star(self);
    }

    pub fn write_count_start(&mut self, distinct: bool) {
        D::write_count_start(distinct, self);
    }

    pub fn write_function_start(&mut self, name: &str) {
        D::write_function_start(name, self);
    }

    pub fn write_operator(&mut self, operator: &str) {
        D::write_operator(operator, self);
    }

    pub fn write_not(&mut self) {
        D::write_not(self);
    }

    pub fn write_in(&mut self, negated: bool) {
        D::write_in(negated, self);
    }

    pub fn write_is_null(&mut self, negated: bool) {
        D::write_is_null(negated, self);
    }

    pub fn write_like(&mut self) {
        D::write_like(self);
    }

    pub fn write_boolean_literal(&mut self, value: bool) {
        D::write_boolean_literal(value, self);
    }

    pub fn write_group_start(&mut self) {
        D::write_group_start(self);
    }

    pub fn write_group_end(&mut self) {
        D::write_group_end(self);
    }

    pub fn write_identifier_separator(&mut self) {
        D::write_identifier_separator(self);
    }

    pub fn write_space(&mut self) {
        D::write_space(self);
    }

    pub fn bind<T>(&mut self, value: T) -> Result<(), Error>
    where
        T: for<'q> Encode<'q, D::DB> + Type<D::DB>,
    {
        self.arguments.add(value).map_err(Error::Encode)?;
        self.arguments
            .format_placeholder(&mut self.sql)
            .map_err(|error| Error::Encode(Box::new(error)))?;
        Ok(())
    }

    pub fn finish(self) -> (String, <D::DB as Database>::Arguments) {
        (self.sql, self.arguments)
    }

    pub fn dialect(&self) -> &D {
        self.dialect
    }
}

/// A renderable SQL expression node.
pub trait RenderExpr<D: Dialect>: Send {
    fn render(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<(), Error>;
}

/// A typed SQL expression.
pub struct Expr<T, D: Dialect> {
    node: Box<dyn RenderExpr<D>>,
    _marker: PhantomData<fn() -> (T, D)>,
}

impl<T, D: Dialect> Expr<T, D> {
    pub fn new(node: impl RenderExpr<D> + 'static) -> Self {
        Self {
            node: Box::new(node),
            _marker: PhantomData,
        }
    }

    pub fn column(table: impl Into<String>, column: impl Into<String>) -> Self {
        Self::qualified_column(None, table, column)
    }

    pub fn qualified_column(
        schema: Option<&str>,
        table: impl Into<String>,
        column: impl Into<String>,
    ) -> Self {
        Self::aliased_column(schema, table, None, column)
    }

    pub fn aliased_column(
        schema: Option<&str>,
        table: impl Into<String>,
        alias: Option<&str>,
        column: impl Into<String>,
    ) -> Self {
        Self::new(ColumnExpr {
            schema: schema.map(str::to_owned),
            table: table.into(),
            alias: alias.map(str::to_owned),
            column: column.into(),
        })
    }

    pub fn value<V>(value: V) -> Self
    where
        V: Clone + for<'q> Encode<'q, D::DB> + Type<D::DB> + Send + 'static,
    {
        Self::new(BindExpr { value })
    }

    pub fn bind(value: T) -> Self
    where
        T: Clone + for<'q> Encode<'q, D::DB> + Type<D::DB> + Send + 'static,
    {
        Self::value(value)
    }

    pub fn binary<L, R>(
        left: Expr<L, D>,
        operator: &'static str,
        right: Expr<R, D>,
    ) -> Expr<bool, D> {
        Expr::new(BinaryExpr {
            left: left.into_node(),
            operator,
            right: right.into_node(),
        })
    }

    pub fn null_check(operand: Expr<T, D>, negated: bool) -> Expr<bool, D> {
        Expr::new(IsNullExpr {
            operand: operand.into_node(),
            negated,
        })
    }

    pub fn in_subquery<L, S>(left: Expr<L, D>, subquery: S, negated: bool) -> Expr<bool, D>
    where
        S: crate::query::Subquery<D> + 'static,
    {
        Expr::new(InSubqueryExpr {
            left: left.into_node(),
            subquery: Box::new(subquery),
            negated,
        })
    }

    pub fn is_in<L>(left: Expr<L, D>, operands: Vec<Expr<T, D>>, negated: bool) -> Expr<bool, D> {
        if operands.is_empty() {
            return Expr::new(LiteralBoolExpr { value: negated });
        }

        Expr::new(InListExpr {
            left: left.into_node(),
            operands: operands.into_iter().map(Expr::into_node).collect(),
            negated,
        })
    }

    pub fn into_order(self, descending: bool) -> Order<D> {
        Order {
            node: self.into_node(),
            descending,
        }
    }

    pub fn erase(self) -> Expr<(), D> {
        Expr {
            node: self.node,
            _marker: PhantomData,
        }
    }

    pub fn function(name: &'static str, args: Vec<Expr<(), D>>) -> Self {
        Self::new(FunctionCallExpr { name, args })
    }

    pub fn count(self) -> Expr<i64, D> {
        Expr::new(CountExpr {
            operand: self.into_node(),
            distinct: false,
        })
    }

    pub fn count_distinct(self) -> Expr<i64, D> {
        Expr::new(CountExpr {
            operand: self.into_node(),
            distinct: true,
        })
    }

    pub fn infix<L, R>(left: Expr<L, D>, operator: &'static str, right: Expr<R, D>) -> Self {
        Self::new(BinaryExpr {
            left: left.into_node(),
            operator,
            right: right.into_node(),
        })
    }

    pub fn prefix(operator: &'static str, operand: Expr<(), D>) -> Self {
        Self::new(PrefixExpr {
            operator,
            operand: operand.into_node(),
        })
    }

    pub fn postfix(operand: Expr<(), D>, operator: &'static str) -> Self {
        Self::new(PostfixExpr {
            operand: operand.into_node(),
            operator,
        })
    }

    pub fn raw(sql: impl Into<String>) -> Self {
        Self::new(RawExpr { sql: sql.into() })
    }

    pub fn call<F>(args: F::Args) -> Expr<F::Output, D>
    where
        F: Function<D> + 'static,
    {
        Expr::new(FunctionExpr::<D, F> {
            args,
            _marker: PhantomData,
        })
    }

    /// Turns a function/aggregate expression into a window function.
    pub fn over(mut self, window: Window<D>) -> Self {
        self.node = Box::new(WindowExpr {
            operand: self.node,
            window,
        });
        self
    }

    pub(crate) fn into_node(self) -> Box<dyn RenderExpr<D>> {
        self.node
    }

    pub fn render(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<(), Error> {
        self.node.render(writer)
    }
}

impl<D: Dialect> Expr<bool, D> {
    pub fn and(self, other: Expr<bool, D>) -> Self {
        Expr::new(LogicalExpr {
            left: self.into_node(),
            operator: "AND",
            right: other.into_node(),
        })
    }

    pub fn or(self, other: Expr<bool, D>) -> Self {
        Expr::new(LogicalExpr {
            left: self.into_node(),
            operator: "OR",
            right: other.into_node(),
        })
    }

    #[allow(clippy::should_implement_trait)]
    pub fn not(self) -> Self {
        Expr::new(NotExpr {
            operand: self.into_node(),
        })
    }
}

impl<D: Dialect> std::ops::Not for Expr<bool, D> {
    type Output = Self;

    fn not(self) -> Self::Output {
        Expr::new(NotExpr {
            operand: self.into_node(),
        })
    }
}

impl<T, D: Dialect> Expr<T, D> {
    pub fn asc(self) -> Order<D> {
        self.into_order(false)
    }

    pub fn desc(self) -> Order<D> {
        self.into_order(true)
    }

    pub fn eq<V>(self, value: V) -> Expr<bool, D>
    where
        V: IntoExpr<T, D>,
    {
        Expr::<bool, D>::binary(self, "=", value.into_expr())
    }

    pub fn ne<V>(self, value: V) -> Expr<bool, D>
    where
        V: IntoExpr<T, D>,
    {
        Expr::<bool, D>::binary(self, "<>", value.into_expr())
    }

    pub fn gt<V>(self, value: V) -> Expr<bool, D>
    where
        V: IntoExpr<T, D>,
    {
        Expr::<bool, D>::binary(self, ">", value.into_expr())
    }

    pub fn ge<V>(self, value: V) -> Expr<bool, D>
    where
        V: IntoExpr<T, D>,
    {
        Expr::<bool, D>::binary(self, ">=", value.into_expr())
    }

    pub fn lt<V>(self, value: V) -> Expr<bool, D>
    where
        V: IntoExpr<T, D>,
    {
        Expr::<bool, D>::binary(self, "<", value.into_expr())
    }

    pub fn le<V>(self, value: V) -> Expr<bool, D>
    where
        V: IntoExpr<T, D>,
    {
        Expr::<bool, D>::binary(self, "<=", value.into_expr())
    }

    pub fn like<V>(self, value: V) -> Expr<bool, D>
    where
        V: IntoExpr<T, D>,
    {
        Expr::<bool, D>::binary(self, "LIKE", value.into_expr())
    }

    pub fn is_null(self) -> Expr<bool, D> {
        Expr::null_check(self, false)
    }

    pub fn is_not_null(self) -> Expr<bool, D> {
        Expr::null_check(self, true)
    }

    pub fn in_list<I, V>(self, values: I) -> Expr<bool, D>
    where
        I: IntoIterator<Item = V>,
        V: IntoExpr<T, D>,
    {
        let operands = values
            .into_iter()
            .map(IntoExpr::into_expr)
            .collect::<Vec<_>>();
        Expr::is_in(self, operands, false)
    }

    pub fn not_in_list<I, V>(self, values: I) -> Expr<bool, D>
    where
        I: IntoIterator<Item = V>,
        V: IntoExpr<T, D>,
    {
        let operands = values
            .into_iter()
            .map(IntoExpr::into_expr)
            .collect::<Vec<_>>();
        Expr::is_in(self, operands, true)
    }
}

impl<T, D: Dialect> RenderExpr<D> for Expr<T, D> {
    fn render(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<(), Error> {
        self.node.render(writer)
    }
}

pub struct ColumnExpr {
    schema: Option<String>,
    table: String,
    alias: Option<String>,
    column: String,
}

impl<D: Dialect> RenderExpr<D> for ColumnExpr {
    fn render(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<(), Error> {
        if let Some(alias) = &self.alias {
            writer.push_ident(alias);
        } else {
            if let Some(schema) = &self.schema {
                writer.push_ident(schema);
                writer.write_identifier_separator();
            }
            writer.push_ident(&self.table);
        }
        writer.write_identifier_separator();
        writer.push_ident(&self.column);
        Ok(())
    }
}

impl<D: Dialect> FunctionArgs<D> for ColumnExpr {
    fn render(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<(), Error> {
        RenderExpr::<D>::render(self, writer)
    }
}

pub struct BindExpr<T> {
    value: T,
}

impl<D: Dialect, T> RenderExpr<D> for BindExpr<T>
where
    T: Clone + for<'q> Encode<'q, D::DB> + Type<D::DB> + Send,
{
    fn render(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<(), Error> {
        writer.bind(self.value.clone())
    }
}

pub struct NotExpr<D: Dialect> {
    operand: Box<dyn RenderExpr<D>>,
}

impl<D: Dialect> RenderExpr<D> for NotExpr<D> {
    fn render(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<(), Error> {
        writer.write_not();
        self.operand.render(writer)?;
        writer.write_group_end();
        Ok(())
    }
}

pub struct LogicalExpr<D: Dialect> {
    left: Box<dyn RenderExpr<D>>,
    operator: &'static str,
    right: Box<dyn RenderExpr<D>>,
}

impl<D: Dialect> RenderExpr<D> for LogicalExpr<D> {
    fn render(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<(), Error> {
        writer.write_group_start();
        self.left.render(writer)?;
        writer.write_group_end();
        match self.operator {
            "AND" => writer.write_and(),
            "OR" => writer.write_or(),
            other => writer.write_operator(other),
        }
        writer.write_group_start();
        self.right.render(writer)?;
        writer.write_group_end();
        Ok(())
    }
}

pub struct CountExpr<D: Dialect> {
    operand: Box<dyn RenderExpr<D>>,
    distinct: bool,
}

impl<D: Dialect> RenderExpr<D> for CountExpr<D> {
    fn render(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<(), Error> {
        writer.write_count_start(self.distinct);
        self.operand.render(writer)?;
        writer.write_group_end();
        Ok(())
    }
}

/// `COUNT(*)`.
pub fn count_star<D: Dialect>() -> Expr<i64, D> {
    Expr::new(CountExpr {
        operand: Box::new(RawExpr {
            sql: "*".to_owned(),
        }),
        distinct: false,
    })
}

pub struct PrefixExpr<D: Dialect> {
    operator: &'static str,
    operand: Box<dyn RenderExpr<D>>,
}

impl<D: Dialect> RenderExpr<D> for PrefixExpr<D> {
    fn render(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<(), Error> {
        writer.push(self.operator);
        writer.write_space();
        self.operand.render(writer)
    }
}

pub struct PostfixExpr<D: Dialect> {
    operand: Box<dyn RenderExpr<D>>,
    operator: &'static str,
}

impl<D: Dialect> RenderExpr<D> for PostfixExpr<D> {
    fn render(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<(), Error> {
        self.operand.render(writer)?;
        writer.write_space();
        writer.push(self.operator);
        Ok(())
    }
}

pub struct FunctionCallExpr<D: Dialect> {
    name: &'static str,
    args: Vec<Expr<(), D>>,
}

impl<D: Dialect> RenderExpr<D> for FunctionCallExpr<D> {
    fn render(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<(), Error> {
        writer.write_function_start(self.name);
        for (index, arg) in self.args.iter_mut().enumerate() {
            if index > 0 {
                writer.write_comma();
            }
            arg.render(writer)?;
        }
        writer.write_group_end();
        Ok(())
    }
}

pub struct BinaryExpr<D: Dialect> {
    left: Box<dyn RenderExpr<D>>,
    operator: &'static str,
    right: Box<dyn RenderExpr<D>>,
}

impl<D: Dialect> RenderExpr<D> for BinaryExpr<D> {
    fn render(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<(), Error> {
        self.left.render(writer)?;
        writer.write_operator(self.operator);
        self.right.render(writer)
    }
}

pub struct IsNullExpr<D: Dialect> {
    operand: Box<dyn RenderExpr<D>>,
    negated: bool,
}

impl<D: Dialect> RenderExpr<D> for IsNullExpr<D> {
    fn render(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<(), Error> {
        self.operand.render(writer)?;
        writer.write_is_null(self.negated);
        Ok(())
    }
}

pub struct InSubqueryExpr<D: Dialect> {
    left: Box<dyn RenderExpr<D>>,
    subquery: Box<dyn crate::query::Subquery<D>>,
    negated: bool,
}

impl<D: Dialect> RenderExpr<D> for InSubqueryExpr<D> {
    fn render(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<(), Error> {
        self.left.render(writer)?;
        writer.push(if self.negated { " NOT IN (" } else { " IN (" });
        self.subquery.render(writer)?;
        writer.write_group_end();
        Ok(())
    }
}

pub struct InListExpr<D: Dialect> {
    left: Box<dyn RenderExpr<D>>,
    operands: Vec<Box<dyn RenderExpr<D>>>,
    negated: bool,
}

impl<D: Dialect> RenderExpr<D> for InListExpr<D> {
    fn render(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<(), Error> {
        self.left.render(writer)?;
        writer.write_in(self.negated);
        for (index, operand) in self.operands.iter_mut().enumerate() {
            if index > 0 {
                writer.write_comma();
            }
            operand.render(writer)?;
        }
        writer.write_group_end();
        Ok(())
    }
}

pub struct LiteralBoolExpr {
    value: bool,
}

impl<D: Dialect> RenderExpr<D> for LiteralBoolExpr {
    fn render(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<(), Error> {
        writer.write_boolean_literal(self.value);
        Ok(())
    }
}

pub struct RawExpr {
    sql: String,
}

impl<D: Dialect> RenderExpr<D> for RawExpr {
    fn render(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<(), Error> {
        writer.push(&self.sql);
        Ok(())
    }
}

/// A rendered ORDER BY term.
pub struct Order<D: Dialect> {
    node: Box<dyn RenderExpr<D>>,
    descending: bool,
}

impl<D: Dialect> Order<D> {
    pub fn render(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<(), Error> {
        self.node.render(writer)?;
        if self.descending {
            writer.write_desc();
        } else {
            writer.write_asc();
        }
        Ok(())
    }
}

/// A window specification: `PARTITION BY ... ORDER BY ...`.
pub struct Window<D: Dialect> {
    partition_by: Vec<Expr<(), D>>,
    order_by: Vec<Order<D>>,
}

impl<D: Dialect> Window<D> {
    pub fn new() -> Self {
        Self {
            partition_by: Vec::new(),
            order_by: Vec::new(),
        }
    }

    pub fn partition_by<E, V>(mut self, expression: V) -> Self
    where
        V: IntoExpr<E, D>,
    {
        self.partition_by.push(expression.into_expr().erase());
        self
    }

    pub fn order_by(mut self, order: Order<D>) -> Self {
        self.order_by.push(order);
        self
    }

    pub fn render(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<(), Error> {
        writer.write_over();

        if !self.partition_by.is_empty() {
            writer.write_partition_by();
            for (index, expression) in self.partition_by.iter_mut().enumerate() {
                if index > 0 {
                    writer.write_comma();
                }
                expression.render(writer)?;
            }
        }

        if !self.order_by.is_empty() {
            writer.write_order_by();
            for (index, order) in self.order_by.iter_mut().enumerate() {
                if index > 0 {
                    writer.write_comma();
                }
                order.render(writer)?;
            }
        }

        writer.write_group_end();
        Ok(())
    }
}

impl<D: Dialect> Default for Window<D> {
    fn default() -> Self {
        Self::new()
    }
}

struct WindowExpr<D: Dialect> {
    operand: Box<dyn RenderExpr<D>>,
    window: Window<D>,
}

impl<D: Dialect> RenderExpr<D> for WindowExpr<D> {
    fn render(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<(), Error> {
        self.operand.render(writer)?;
        self.window.render(writer)
    }
}

/// `ROW_NUMBER()`.
pub fn row_number<D: Dialect>() -> Expr<i64, D> {
    Expr::new(FunctionCallExpr {
        name: "ROW_NUMBER",
        args: Vec::new(),
    })
}

/// `RANK()`.
pub fn rank<D: Dialect>() -> Expr<i64, D> {
    Expr::new(FunctionCallExpr {
        name: "RANK",
        args: Vec::new(),
    })
}

/// `DENSE_RANK()`.
pub fn dense_rank<D: Dialect>() -> Expr<i64, D> {
    Expr::new(FunctionCallExpr {
        name: "DENSE_RANK",
        args: Vec::new(),
    })
}

/// Arguments accepted by a SQL function.
pub trait FunctionArgs<D: Dialect>: Send {
    fn render(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<(), Error>;
}

impl<D: Dialect> FunctionArgs<D> for () {
    fn render(&mut self, _writer: &mut SqlWriter<'_, D>) -> Result<(), Error> {
        Ok(())
    }
}

impl<D: Dialect, T> FunctionArgs<D> for Expr<T, D> {
    fn render(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<(), Error> {
        self.render(writer)
    }
}

impl<D: Dialect, A> FunctionArgs<D> for (A,)
where
    A: FunctionArgs<D>,
{
    fn render(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<(), Error> {
        self.0.render(writer)
    }
}

impl<D: Dialect, A, B> FunctionArgs<D> for (A, B)
where
    A: FunctionArgs<D>,
    B: FunctionArgs<D>,
{
    fn render(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<(), Error> {
        self.0.render(writer)?;
        writer.write_comma();
        self.1.render(writer)
    }
}

impl<D: Dialect, A, B, C> FunctionArgs<D> for (A, B, C)
where
    A: FunctionArgs<D>,
    B: FunctionArgs<D>,
    C: FunctionArgs<D>,
{
    fn render(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<(), Error> {
        self.0.render(writer)?;
        writer.write_comma();
        self.1.render(writer)?;
        writer.write_comma();
        self.2.render(writer)
    }
}

/// Converts either a typed expression or a Rust value into an expression.
pub trait IntoExpr<T, D: Dialect> {
    fn into_expr(self) -> Expr<T, D>;
}

impl<T, D: Dialect> IntoExpr<T, D> for Expr<T, D> {
    fn into_expr(self) -> Expr<T, D> {
        self
    }
}

impl<T, D, V> IntoExpr<T, D> for V
where
    D: Dialect,
    V: Clone + for<'q> Encode<'q, D::DB> + Type<D::DB> + Send + 'static,
{
    fn into_expr(self) -> Expr<T, D> {
        Expr::value(self)
    }
}

/// `COALESCE(left, right)` with the arguments' expression type.
pub fn coalesce<D, T, A, B>(left: A, right: B) -> Expr<T, D>
where
    D: Dialect,
    A: IntoExpr<T, D>,
    B: IntoExpr<T, D>,
{
    Expr::function(
        "COALESCE",
        vec![left.into_expr().erase(), right.into_expr().erase()],
    )
}

/// A SQL function that can be rendered for a specific dialect.
pub trait Function<D: Dialect> {
    type Args: FunctionArgs<D>;
    type Output;
    const NAME: &'static str;

    fn render(args: &mut Self::Args, writer: &mut SqlWriter<'_, D>) -> Result<(), Error> {
        writer.push(Self::NAME);
        writer.write_group_start();
        args.render(writer)?;
        writer.write_group_end();
        Ok(())
    }
}

/// Calls a typed SQL function.
pub fn call<D: Dialect, F: Function<D> + 'static>(args: F::Args) -> Expr<F::Output, D> {
    Expr::new(FunctionExpr::<D, F> {
        args,
        _marker: PhantomData,
    })
}

struct FunctionExpr<D: Dialect, F: Function<D>> {
    args: F::Args,
    _marker: PhantomData<D>,
}

impl<D, F> RenderExpr<D> for FunctionExpr<D, F>
where
    D: Dialect,
    F: Function<D>,
{
    fn render(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<(), Error> {
        F::render(&mut self.args, writer)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fixity {
    Infix,
    Prefix,
    Postfix,
}

/// A SQL operator that can be rendered for a specific dialect.
pub trait Operator<D: Dialect> {
    type Left: FunctionArgs<D>;
    type Right: FunctionArgs<D>;
    type Output;
    const SQL: &'static str;
    const FIXITY: Fixity;
}

pub struct OperatorExpr<O: Operator<D>, D: Dialect> {
    pub left: O::Left,
    pub right: O::Right,
    _marker: PhantomData<fn() -> (O, D)>,
}

impl<O, D> RenderExpr<D> for OperatorExpr<O, D>
where
    O: Operator<D>,
    D: Dialect,
{
    fn render(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<(), Error> {
        match O::FIXITY {
            Fixity::Infix => {
                self.left.render(writer)?;
                writer.write_operator(O::SQL);
                self.right.render(writer)
            }
            Fixity::Prefix => {
                writer.push(O::SQL);
                writer.write_space();
                self.right.render(writer)
            }
            Fixity::Postfix => {
                self.left.render(writer)?;
                writer.write_space();
                writer.push(O::SQL);
                Ok(())
            }
        }
    }
}
