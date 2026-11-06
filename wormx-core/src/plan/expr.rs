//! Backend-neutral expression AST.
//!
//! The AST is generic over [`Dialect`], not over SQL. Each dialect supplies a
//! renderer that interprets [`ExprKind`] into its own query language. The SQL
//! renderer lives in [`crate::sql::expr`].

use std::marker::PhantomData;

use sqlx_core::encode::Encode;
use sqlx_core::error::Error;
use sqlx_core::types::Type;

use super::backend::Dialect;
use super::command::Writer;

use super::query::{Column, Subquery};

/// A qualified column reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnRef {
    pub schema: Option<String>,
    pub table: String,
    pub alias: Option<String>,
    pub column: String,
}

impl ColumnRef {
    pub fn new(
        schema: Option<impl Into<String>>,
        table: impl Into<String>,
        column: impl Into<String>,
    ) -> Self {
        Self {
            schema: schema.map(Into::into),
            table: table.into(),
            alias: None,
            column: column.into(),
        }
    }

    pub fn with_alias(
        schema: Option<impl Into<String>>,
        table: impl Into<String>,
        alias: Option<impl Into<String>>,
        column: impl Into<String>,
    ) -> Self {
        Self {
            schema: schema.map(Into::into),
            table: table.into(),
            alias: alias.map(Into::into),
            column: column.into(),
        }
    }
}

/// Compatibility alias for the old name.
pub type ColumnExpr = ColumnRef;

/// A value that can be written into a query as a bound argument.
///
/// SQLx's `Encode` and `Type` traits are database-level, not SQL-level, so a
/// non-SQL backend can implement this for its own value types. The blanket
/// implementation covers every SQLx value type.
pub trait BindValue<D: Dialect>: Send {
    fn write(&self, writer: &mut Writer<D>) -> Result<(), Error>;
}

impl<D, T> BindValue<D> for T
where
    D: Dialect,
    T: Clone + for<'q> Encode<'q, D::DB> + Type<D::DB> + Send + 'static,
{
    fn write(&self, writer: &mut Writer<D>) -> Result<(), Error> {
        writer.bind(self.clone())
    }
}

/// A custom expression node supplied by a dialect or extension crate.
pub trait CustomNode<D: Dialect>: Send {
    fn render(&mut self, writer: &mut Writer<D>) -> Result<(), Error>;
}

/// A logical connective.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicalOp {
    And,
    Or,
}

/// The backend-neutral expression tree.
#[non_exhaustive]
pub enum ExprKind<D: Dialect> {
    Column(ColumnRef),
    Bind(Box<dyn BindValue<D>>),
    Not(Box<Expr<(), D>>),
    Group(Box<Expr<(), D>>),
    Logical {
        op: LogicalOp,
        left: Box<Expr<(), D>>,
        right: Box<Expr<(), D>>,
    },
    Count {
        operand: Box<Expr<(), D>>,
        distinct: bool,
    },
    Prefix {
        operator: &'static str,
        operand: Box<Expr<(), D>>,
        group_operand: bool,
    },
    Postfix {
        operand: Box<Expr<(), D>>,
        operator: &'static str,
        group_operand: bool,
    },
    Function {
        name: &'static str,
        args: Vec<Expr<(), D>>,
    },
    Binary {
        operator: &'static str,
        left: Box<Expr<(), D>>,
        right: Box<Expr<(), D>>,
        group_operands: bool,
    },
    IsNull {
        operand: Box<Expr<(), D>>,
        negated: bool,
    },
    InSubquery {
        left: Box<Expr<(), D>>,
        subquery: Box<dyn Subquery<D>>,
        negated: bool,
    },
    InList {
        left: Box<Expr<(), D>>,
        operands: Vec<Expr<(), D>>,
        negated: bool,
    },
    LiteralBool(bool),
    Raw(String),
    Window {
        operand: Box<Expr<(), D>>,
        window: Window<D>,
    },
    /// A `CASE WHEN ... THEN ... ELSE ... END` expression.
    Case {
        branches: Vec<(Expr<(), D>, Expr<(), D>)>,
        otherwise: Option<Box<Expr<(), D>>>,
    },
    /// A `CAST(operand AS to)` expression.
    Cast {
        operand: Box<Expr<(), D>>,
        /// A SQL type name such as `"NVARCHAR(200)"`. It is written verbatim,
        /// so it must be a constant, never user input.
        to: &'static str,
    },
    /// The value supplied for a column by the incoming row of an upsert.
    ///
    /// SQL renders this as `excluded.column` (PostgreSQL/SQLite),
    /// `VALUES(column)` (MySQL), or `source.column` (SQL Server). It is only
    /// meaningful inside an upsert's update clause.
    InputColumn(ColumnRef),
    Custom(Box<dyn CustomNode<D>>),
}

/// A `CASE` expression under construction.
///
/// Start one with [`Expr::case`], add branches with [`Case::when`], and finish
/// with either [`Case::otherwise`] or [`Case::end`].
pub struct Case<T, D: Dialect> {
    branches: Vec<(Expr<(), D>, Expr<(), D>)>,
    _marker: PhantomData<fn() -> T>,
}

impl<T, D: Dialect> Case<T, D> {
    /// Adds a `WHEN condition THEN value` branch.
    pub fn when<C, V>(mut self, condition: C, value: V) -> Self
    where
        C: IntoExpr<bool, D>,
        V: IntoExpr<T, D>,
    {
        self.branches
            .push((condition.into_expr().erase(), value.into_expr().erase()));
        self
    }

    /// Finishes the expression with `ELSE value`.
    pub fn otherwise<V>(self, value: V) -> Expr<T, D>
    where
        V: IntoExpr<T, D>,
    {
        Expr::from_kind(ExprKind::Case {
            branches: self.branches,
            otherwise: Some(Box::new(value.into_expr().erase())),
        })
    }

    /// Finishes the expression without an `ELSE`, producing `NULL` when no
    /// branch matches.
    pub fn end(self) -> Expr<Option<T>, D> {
        Expr::from_kind(ExprKind::Case {
            branches: self.branches,
            otherwise: None,
        })
    }
}

/// A typed expression.
pub struct Expr<T, D: Dialect> {
    kind: ExprKind<D>,
    _marker: PhantomData<fn() -> T>,
}

impl<T, D: Dialect> Expr<T, D> {
    pub fn new(node: impl CustomNode<D> + 'static) -> Self {
        Self::from_kind(ExprKind::Custom(Box::new(node)))
    }

    pub fn from_kind(kind: ExprKind<D>) -> Self {
        Self {
            kind,
            _marker: PhantomData,
        }
    }

    pub fn kind(&self) -> &ExprKind<D> {
        &self.kind
    }

    pub fn kind_mut(&mut self) -> &mut ExprKind<D> {
        &mut self.kind
    }

    pub fn into_kind(self) -> ExprKind<D> {
        self.kind
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
        Self::from_kind(ExprKind::Column(ColumnRef::with_alias(
            schema, table, alias, column,
        )))
    }

    pub fn value<V>(value: V) -> Self
    where
        V: BindValue<D> + 'static,
    {
        Self::from_kind(ExprKind::Bind(Box::new(value)))
    }

    /// Places a bound value where the builder has no typed method.
    ///
    /// This is the composition-safe alternative to [`Expr::raw`] for values.
    /// The value is encoded and rendered as a placeholder, so it cannot change
    /// the query text.
    pub fn bind(value: T) -> Self
    where
        T: BindValue<D> + 'static,
    {
        Self::value(value)
    }

    pub fn binary<L, R>(
        left: Expr<L, D>,
        operator: &'static str,
        right: Expr<R, D>,
    ) -> Expr<bool, D> {
        Expr::from_kind(ExprKind::Binary {
            left: Box::new(left.erase()),
            operator,
            right: Box::new(right.erase()),
            group_operands: true,
        })
    }

    pub fn null_check(operand: Expr<T, D>, negated: bool) -> Expr<bool, D> {
        Expr::from_kind(ExprKind::IsNull {
            operand: Box::new(operand.erase()),
            negated,
        })
    }

    pub fn in_subquery<L, S>(left: Expr<L, D>, subquery: S, negated: bool) -> Expr<bool, D>
    where
        S: Subquery<D> + 'static,
    {
        Expr::from_kind(ExprKind::InSubquery {
            left: Box::new(left.erase()),
            subquery: Box::new(subquery),
            negated,
        })
    }

    pub fn is_in<L>(left: Expr<L, D>, operands: Vec<Expr<T, D>>, negated: bool) -> Expr<bool, D> {
        if operands.is_empty() {
            return Expr::from_kind(ExprKind::LiteralBool(negated));
        }

        Expr::from_kind(ExprKind::InList {
            left: Box::new(left.erase()),
            operands: operands.into_iter().map(Expr::erase).collect(),
            negated,
        })
    }

    pub fn into_order(self, descending: bool) -> Order<D> {
        Order {
            node: Box::new(self.erase()),
            descending,
        }
    }

    pub fn erase(self) -> Expr<(), D> {
        Expr {
            kind: self.kind,
            _marker: PhantomData,
        }
    }

    pub fn function(name: &'static str, args: Vec<Expr<(), D>>) -> Self {
        Self::from_kind(ExprKind::Function { name, args })
    }

    pub fn count(self) -> Expr<i64, D> {
        Expr::from_kind(ExprKind::Count {
            operand: Box::new(self.erase()),
            distinct: false,
        })
    }

    pub fn count_distinct(self) -> Expr<i64, D> {
        Expr::from_kind(ExprKind::Count {
            operand: Box::new(self.erase()),
            distinct: true,
        })
    }

    /// Starts a `CASE` expression.
    ///
    /// ```ignore
    /// let flag = Expr::<i32, MssqlDialect>::case()
    ///     .when(Users::fields().active().eq(true), 1)
    ///     .otherwise(0);
    /// ```
    pub fn case() -> Case<T, D> {
        Case {
            branches: Vec::new(),
            _marker: PhantomData,
        }
    }

    /// Refers to the value supplied for `column` by the incoming upsert row.
    ///
    /// The dialect renders this as `excluded.column`, `VALUES(column)`, or
    /// `source.column`. It is only meaningful in an upsert's update clause.
    pub fn input(column: Column<T>) -> Self {
        Self::from_kind(ExprKind::InputColumn(column.column_ref()))
    }

    /// Casts this expression to the SQL type named by `to`.
    ///
    /// `to` is written into the query verbatim, so it must be a constant such
    /// as `"NVARCHAR(200)"` or `"DECIMAL(10, 2)"`. Never pass user input.
    pub fn cast(self, to: &'static str) -> Self {
        Self::from_kind(ExprKind::Cast {
            operand: Box::new(self.erase()),
            to,
        })
    }

    pub fn infix<L, R>(left: Expr<L, D>, operator: &'static str, right: Expr<R, D>) -> Self {
        Self::from_kind(ExprKind::Binary {
            left: Box::new(left.erase()),
            operator,
            right: Box::new(right.erase()),
            group_operands: true,
        })
    }

    /// Like [`Expr::infix`], but without implicit operand parentheses.
    pub fn infix_ungrouped<L, R>(
        left: Expr<L, D>,
        operator: &'static str,
        right: Expr<R, D>,
    ) -> Self {
        Self::from_kind(ExprKind::Binary {
            left: Box::new(left.erase()),
            operator,
            right: Box::new(right.erase()),
            group_operands: false,
        })
    }

    pub fn prefix(operator: &'static str, operand: Expr<(), D>) -> Self {
        Self::from_kind(ExprKind::Prefix {
            operator,
            operand: Box::new(operand),
            group_operand: true,
        })
    }

    /// Like [`Expr::prefix`], but without implicit operand parentheses.
    pub fn prefix_ungrouped(operator: &'static str, operand: Expr<(), D>) -> Self {
        Self::from_kind(ExprKind::Prefix {
            operator,
            operand: Box::new(operand),
            group_operand: false,
        })
    }

    pub fn postfix(operand: Expr<(), D>, operator: &'static str) -> Self {
        Self::from_kind(ExprKind::Postfix {
            operand: Box::new(operand),
            operator,
            group_operand: true,
        })
    }

    /// Like [`Expr::postfix`], but without implicit operand parentheses.
    pub fn postfix_ungrouped(operand: Expr<(), D>, operator: &'static str) -> Self {
        Self::from_kind(ExprKind::Postfix {
            operand: Box::new(operand),
            operator,
            group_operand: false,
        })
    }

    /// Wraps this expression in parentheses.
    pub fn group(self) -> Self {
        Self::from_kind(ExprKind::Group(Box::new(self.erase())))
    }

    /// Emits a raw query fragment verbatim, without binding it.
    pub fn raw(sql: impl Into<String>) -> Self {
        Self::from_kind(ExprKind::Raw(sql.into()))
    }

    /// Turns a function/aggregate expression into a window function.
    pub fn over(self, window: Window<D>) -> Self {
        Self::from_kind(ExprKind::Window {
            operand: Box::new(self.erase()),
            window,
        })
    }
}

impl<D: Dialect> Expr<bool, D> {
    pub fn and(self, other: Expr<bool, D>) -> Self {
        Expr::from_kind(ExprKind::Logical {
            op: LogicalOp::And,
            left: Box::new(self.erase()),
            right: Box::new(other.erase()),
        })
    }

    pub fn or(self, other: Expr<bool, D>) -> Self {
        Expr::from_kind(ExprKind::Logical {
            op: LogicalOp::Or,
            left: Box::new(self.erase()),
            right: Box::new(other.erase()),
        })
    }

    #[allow(clippy::should_implement_trait)]
    pub fn not(self) -> Self {
        Expr::from_kind(ExprKind::Not(Box::new(self.erase())))
    }
}

impl<D: Dialect> std::ops::Not for Expr<bool, D> {
    type Output = Self;

    fn not(self) -> Self::Output {
        Expr::from_kind(ExprKind::Not(Box::new(self.erase())))
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

/// A rendered `ORDER BY` term.
pub struct Order<D: Dialect> {
    node: Box<Expr<(), D>>,
    descending: bool,
}

impl<D: Dialect> Order<D> {
    pub fn expr(&self) -> &Expr<(), D> {
        &self.node
    }

    pub fn expr_mut(&mut self) -> &mut Expr<(), D> {
        &mut self.node
    }

    pub fn descending(&self) -> bool {
        self.descending
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

    pub fn partition_by_items(&self) -> &[Expr<(), D>] {
        &self.partition_by
    }

    pub fn partition_by_items_mut(&mut self) -> &mut [Expr<(), D>] {
        &mut self.partition_by
    }

    pub fn order_by_items(&self) -> &[Order<D>] {
        &self.order_by
    }

    pub fn order_by_items_mut(&mut self) -> &mut [Order<D>] {
        &mut self.order_by
    }
}

impl<D: Dialect> Default for Window<D> {
    fn default() -> Self {
        Self::new()
    }
}

/// `COUNT(*)`.
pub fn count_star<D: Dialect>() -> Expr<i64, D> {
    Expr::from_kind(ExprKind::Count {
        operand: Box::new(Expr::raw("*")),
        distinct: false,
    })
}

/// `ROW_NUMBER()`.
pub fn row_number<D: Dialect>() -> Expr<i64, D> {
    Expr::function("ROW_NUMBER", Vec::new())
}

/// `RANK()`.
pub fn rank<D: Dialect>() -> Expr<i64, D> {
    Expr::function("RANK", Vec::new())
}

/// `DENSE_RANK()`.
pub fn dense_rank<D: Dialect>() -> Expr<i64, D> {
    Expr::function("DENSE_RANK", Vec::new())
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
