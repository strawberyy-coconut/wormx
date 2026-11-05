//! Typed SQL expressions, functions, and operators.

use std::marker::PhantomData;

use sqlx_core::encode::Encode;
use sqlx_core::error::Error;
use sqlx_core::types::Type;

use super::syntax::Dialect;
use super::writer::SqlWriter;

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

    /// Places a bound value where the builder has no typed method.
    ///
    /// This is the composition-safe alternative to [`Expr::raw`] for values.
    /// The value is encoded and rendered as a placeholder, so it cannot change
    /// the SQL text.
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
            group_operands: true,
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
        S: crate::plan::query::Subquery<D> + 'static,
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
            group_operands: true,
        })
    }

    /// Like [`Expr::infix`], but without implicit operand parentheses.
    ///
    /// This is the escape hatch used by `operator! { grouping = None; }`.
    /// Prefer [`Expr::infix`] unless the operator's SQL grammar forbids the
    /// extra parentheses.
    pub fn infix_ungrouped<L, R>(
        left: Expr<L, D>,
        operator: &'static str,
        right: Expr<R, D>,
    ) -> Self {
        Self::new(BinaryExpr {
            left: left.into_node(),
            operator,
            right: right.into_node(),
            group_operands: false,
        })
    }

    pub fn prefix(operator: &'static str, operand: Expr<(), D>) -> Self {
        Self::new(PrefixExpr {
            operator,
            operand: operand.into_node(),
            group_operand: true,
        })
    }

    /// Like [`Expr::prefix`], but without implicit operand parentheses.
    pub fn prefix_ungrouped(operator: &'static str, operand: Expr<(), D>) -> Self {
        Self::new(PrefixExpr {
            operator,
            operand: operand.into_node(),
            group_operand: false,
        })
    }

    pub fn postfix(operand: Expr<(), D>, operator: &'static str) -> Self {
        Self::new(PostfixExpr {
            operand: operand.into_node(),
            operator,
            group_operand: true,
        })
    }

    /// Like [`Expr::postfix`], but without implicit operand parentheses.
    pub fn postfix_ungrouped(operand: Expr<(), D>, operator: &'static str) -> Self {
        Self::new(PostfixExpr {
            operand: operand.into_node(),
            operator,
            group_operand: false,
        })
    }

    /// Wraps this expression in parentheses.
    ///
    /// Custom operators and hand-built predicates can use this to make their
    /// grouping explicit. The built-in operator macros already group their
    /// operands, so this is only needed when composing raw or third-party
    /// expression nodes.
    pub fn group(self) -> Self {
        Self::new(GroupExpr {
            operand: self.into_node(),
        })
    }

    /// Emits a raw SQL fragment verbatim, without binding it.
    ///
    /// This is for SQL literals and database constructs that must stay literal,
    /// such as a filtered-index predicate (`status <> N'FINISHED'`). Never
    /// interpolate user input into this string:
    ///
    /// ```ignore
    /// // Injection risk:
    /// Expr::raw(format!("name = '{input}'"))
    ///
    /// // Composition-safe:
    /// Users::fields().name().eq(input)
    /// // or, when there is no typed helper:
    /// Expr::bind(input)
    /// ```
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

/// A qualified column reference.
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

/// A bound value placeholder.
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

/// A negated expression.
pub struct NotExpr<D: Dialect> {
    operand: Box<dyn RenderExpr<D>>,
}

impl<D: Dialect> RenderExpr<D> for NotExpr<D> {
    fn render(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<(), Error> {
        writer.write_not();
        writer.write_group_start();
        self.operand.render(writer)?;
        writer.write_group_end();
        Ok(())
    }
}

/// An explicitly parenthesized expression.
pub struct GroupExpr<D: Dialect> {
    operand: Box<dyn RenderExpr<D>>,
}

impl<D: Dialect> RenderExpr<D> for GroupExpr<D> {
    fn render(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<(), Error> {
        writer.write_group_start();
        self.operand.render(writer)?;
        writer.write_group_end();
        Ok(())
    }
}

/// A logical `AND`/`OR` expression.
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

/// A `COUNT` expression.
pub struct CountExpr<D: Dialect> {
    operand: Box<dyn RenderExpr<D>>,
    distinct: bool,
}

impl<D: Dialect> RenderExpr<D> for CountExpr<D> {
    fn render(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<(), Error> {
        writer.write_count_start(self.distinct);
        writer.write_group_start();
        if self.distinct {
            writer.push("DISTINCT ");
        }
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

/// A prefix operator expression.
pub struct PrefixExpr<D: Dialect> {
    operator: &'static str,
    operand: Box<dyn RenderExpr<D>>,
    group_operand: bool,
}

impl<D: Dialect> RenderExpr<D> for PrefixExpr<D> {
    fn render(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<(), Error> {
        writer.push(self.operator);
        writer.write_space();
        if self.group_operand {
            writer.write_group_start();
        }
        self.operand.render(writer)?;
        if self.group_operand {
            writer.write_group_end();
        }
        Ok(())
    }
}

/// A postfix operator expression.
pub struct PostfixExpr<D: Dialect> {
    operand: Box<dyn RenderExpr<D>>,
    operator: &'static str,
    group_operand: bool,
}

impl<D: Dialect> RenderExpr<D> for PostfixExpr<D> {
    fn render(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<(), Error> {
        if self.group_operand {
            writer.write_group_start();
        }
        self.operand.render(writer)?;
        if self.group_operand {
            writer.write_group_end();
        }
        writer.write_space();
        writer.push(self.operator);
        Ok(())
    }
}

/// A SQL function call.
pub struct FunctionCallExpr<D: Dialect> {
    name: &'static str,
    args: Vec<Expr<(), D>>,
}

impl<D: Dialect> RenderExpr<D> for FunctionCallExpr<D> {
    fn render(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<(), Error> {
        writer.write_function_start(self.name);
        writer.write_group_start();
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

/// A binary operator expression.
pub struct BinaryExpr<D: Dialect> {
    left: Box<dyn RenderExpr<D>>,
    operator: &'static str,
    right: Box<dyn RenderExpr<D>>,
    group_operands: bool,
}

impl<D: Dialect> RenderExpr<D> for BinaryExpr<D> {
    fn render(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<(), Error> {
        if self.group_operands {
            writer.write_group_start();
        }
        self.left.render(writer)?;
        if self.group_operands {
            writer.write_group_end();
        }
        writer.write_operator(self.operator);
        if self.group_operands {
            writer.write_group_start();
        }
        self.right.render(writer)?;
        if self.group_operands {
            writer.write_group_end();
        }
        Ok(())
    }
}

/// An `IS NULL`/`IS NOT NULL` expression.
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

/// An `IN (subquery)` expression.
pub struct InSubqueryExpr<D: Dialect> {
    left: Box<dyn RenderExpr<D>>,
    subquery: Box<dyn crate::plan::query::Subquery<D>>,
    negated: bool,
}

impl<D: Dialect> RenderExpr<D> for InSubqueryExpr<D> {
    fn render(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<(), Error> {
        self.left.render(writer)?;
        writer.write_in(self.negated);
        self.subquery.render(writer)?;
        writer.write_group_end();
        Ok(())
    }
}

/// An `IN (...)` expression.
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

/// A literal boolean expression.
pub struct LiteralBoolExpr {
    value: bool,
}

impl<D: Dialect> RenderExpr<D> for LiteralBoolExpr {
    fn render(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<(), Error> {
        writer.write_boolean_literal(self.value);
        Ok(())
    }
}

/// A raw SQL fragment.
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
/// Where an operator is placed relative to its operands.
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

/// A custom operator expression.
pub struct OperatorExpr<O: Operator<D>, D: Dialect> {
    pub left: O::Left,
    pub right: O::Right,
    _marker: PhantomData<fn() -> (O, D)>,
}

impl<O: Operator<D>, D: Dialect> OperatorExpr<O, D> {
    /// Creates an operator expression.
    ///
    /// For prefix operators `left` is normally `()`, and for postfix operators
    /// `right` is normally `()`.
    pub fn new(left: O::Left, right: O::Right) -> Self {
        Self {
            left,
            right,
            _marker: PhantomData,
        }
    }
}

impl<O, D> RenderExpr<D> for OperatorExpr<O, D>
where
    O: Operator<D>,
    D: Dialect,
{
    fn render(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<(), Error> {
        match O::FIXITY {
            Fixity::Infix => {
                writer.write_group_start();
                self.left.render(writer)?;
                writer.write_group_end();
                writer.write_operator(O::SQL);
                writer.write_group_start();
                self.right.render(writer)?;
                writer.write_group_end();
                Ok(())
            }
            Fixity::Prefix => {
                writer.push(O::SQL);
                writer.write_space();
                writer.write_group_start();
                self.right.render(writer)?;
                writer.write_group_end();
                Ok(())
            }
            Fixity::Postfix => {
                writer.write_group_start();
                self.left.render(writer)?;
                writer.write_group_end();
                writer.write_space();
                writer.push(O::SQL);
                Ok(())
            }
        }
    }
}
