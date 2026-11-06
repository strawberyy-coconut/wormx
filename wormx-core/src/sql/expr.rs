//! SQL rendering for the backend-neutral expression AST, plus SQL-only
//! function and operator extension points.

use std::marker::PhantomData;

use sqlx_core::error::Error;

use super::syntax::SqlDialect;
use super::writer::Writer;
use crate::plan::expr::{ColumnRef, CustomNode, Expr, ExprKind, LogicalOp, Order, Window};

/// Renders an expression as SQL.
pub fn render_expr<T, D: SqlDialect>(
    expr: &mut Expr<T, D>,
    writer: &mut Writer<D>,
) -> Result<(), Error> {
    render_expr_kind(expr.kind_mut(), writer)
}

/// Renders an already-erased expression kind as SQL.
pub fn render_expr_kind<D: SqlDialect>(
    kind: &mut ExprKind<D>,
    writer: &mut Writer<D>,
) -> Result<(), Error> {
    match kind {
        ExprKind::Column(column) => {
            render_column_ref(column, writer);
            Ok(())
        }
        ExprKind::Bind(value) => value.write(writer),
        ExprKind::Not(operand) => {
            writer.write_not();
            writer.write_group_start();
            render_expr(operand, writer)?;
            writer.write_group_end();
            Ok(())
        }
        ExprKind::Group(operand) => {
            writer.write_group_start();
            render_expr(operand, writer)?;
            writer.write_group_end();
            Ok(())
        }
        ExprKind::Logical { op, left, right } => {
            writer.write_group_start();
            render_expr(left, writer)?;
            writer.write_group_end();
            match op {
                LogicalOp::And => writer.write_and(),
                LogicalOp::Or => writer.write_or(),
            }
            writer.write_group_start();
            render_expr(right, writer)?;
            writer.write_group_end();
            Ok(())
        }
        ExprKind::Count { operand, distinct } => {
            writer.write_count_start(*distinct);
            writer.write_group_start();
            if *distinct {
                writer.push("DISTINCT ");
            }
            render_expr(operand, writer)?;
            writer.write_group_end();
            Ok(())
        }
        ExprKind::Prefix {
            operator,
            operand,
            group_operand,
        } => {
            writer.push(operator);
            writer.push(" ");
            if *group_operand {
                writer.write_group_start();
            }
            render_expr(operand, writer)?;
            if *group_operand {
                writer.write_group_end();
            }
            Ok(())
        }
        ExprKind::Postfix {
            operand,
            operator,
            group_operand,
        } => {
            if *group_operand {
                writer.write_group_start();
            }
            render_expr(operand, writer)?;
            if *group_operand {
                writer.write_group_end();
            }
            writer.push(" ");
            writer.push(operator);
            Ok(())
        }
        ExprKind::Function { name, args } => {
            writer.write_function_start(name);
            writer.write_group_start();
            for (index, arg) in args.iter_mut().enumerate() {
                if index > 0 {
                    writer.write_comma();
                }
                render_expr(arg, writer)?;
            }
            writer.write_group_end();
            Ok(())
        }
        ExprKind::Binary {
            operator,
            left,
            right,
            group_operands,
        } => {
            if *group_operands {
                writer.write_group_start();
            }
            render_expr(left, writer)?;
            if *group_operands {
                writer.write_group_end();
            }
            writer.write_operator(operator);
            if *group_operands {
                writer.write_group_start();
            }
            render_expr(right, writer)?;
            if *group_operands {
                writer.write_group_end();
            }
            Ok(())
        }
        ExprKind::IsNull { operand, negated } => {
            render_expr(operand, writer)?;
            writer.write_is_null(*negated);
            Ok(())
        }
        ExprKind::InSubquery {
            left,
            subquery,
            negated,
        } => {
            render_expr(left, writer)?;
            writer.write_in(*negated);
            subquery.render(writer)?;
            writer.write_group_end();
            Ok(())
        }
        ExprKind::InList {
            left,
            operands,
            negated,
        } => {
            render_expr(left, writer)?;
            writer.write_in(*negated);
            for (index, operand) in operands.iter_mut().enumerate() {
                if index > 0 {
                    writer.write_comma();
                }
                render_expr(operand, writer)?;
            }
            writer.write_group_end();
            Ok(())
        }
        ExprKind::LiteralBool(value) => {
            writer.write_boolean_literal(*value);
            Ok(())
        }
        ExprKind::Raw(text) => {
            writer.push(text);
            Ok(())
        }
        ExprKind::Window { operand, window } => {
            render_expr(operand, writer)?;
            render_window(window, writer)
        }
        ExprKind::Case {
            branches,
            otherwise,
        } => {
            writer.push("CASE");
            for (condition, value) in branches.iter_mut() {
                writer.push(" WHEN ");
                render_expr(condition, writer)?;
                writer.push(" THEN ");
                render_expr(value, writer)?;
            }
            if let Some(otherwise) = otherwise {
                writer.push(" ELSE ");
                render_expr(otherwise, writer)?;
            }
            writer.push(" END");
            Ok(())
        }
        ExprKind::Cast { operand, to } => {
            writer.push("CAST");
            writer.write_group_start();
            render_expr(operand, writer)?;
            writer.push(" AS ");
            writer.push(to);
            writer.write_group_end();
            Ok(())
        }
        ExprKind::InputColumn(column) => {
            writer.write_input_column(&column.column);
            Ok(())
        }
        ExprKind::Custom(node) => node.render(writer),
    }
}

/// Renders a qualified column reference.
pub fn render_column_ref<D: SqlDialect>(column: &ColumnRef, writer: &mut Writer<D>) {
    if let Some(alias) = &column.alias {
        writer.push_ident(alias);
    } else {
        if let Some(schema) = &column.schema {
            writer.push_ident(schema);
            writer.write_identifier_separator();
        }
        writer.push_ident(&column.table);
    }
    writer.write_identifier_separator();
    writer.push_ident(&column.column);
}

/// Renders an `ORDER BY` term.
pub fn render_order<D: SqlDialect>(
    order: &mut Order<D>,
    writer: &mut Writer<D>,
) -> Result<(), Error> {
    render_expr(order.expr_mut(), writer)?;
    if order.descending() {
        writer.write_desc();
    } else {
        writer.write_asc();
    }
    Ok(())
}

/// Renders a window specification.
pub fn render_window<D: SqlDialect>(
    window: &mut Window<D>,
    writer: &mut Writer<D>,
) -> Result<(), Error> {
    writer.write_over();

    if !window.partition_by_items().is_empty() {
        writer.write_partition_by();
        for (index, expression) in window.partition_by_items_mut().iter_mut().enumerate() {
            if index > 0 {
                writer.write_comma();
            }
            render_expr(expression, writer)?;
        }
    }

    if !window.order_by_items().is_empty() {
        writer.write_order_by();
        for (index, order) in window.order_by_items_mut().iter_mut().enumerate() {
            if index > 0 {
                writer.write_comma();
            }
            render_order(order, writer)?;
        }
    }

    writer.write_group_end();
    Ok(())
}

impl<T, D: SqlDialect> Expr<T, D> {
    /// Renders this expression as SQL.
    pub fn render(&mut self, writer: &mut Writer<D>) -> Result<(), Error> {
        render_expr(self, writer)
    }
}

/// Arguments accepted by a SQL function.
///
/// Tuples support up to 16 arguments.
pub trait FunctionArgs<D: SqlDialect>: Send {
    fn render(&mut self, writer: &mut Writer<D>) -> Result<(), Error>;
}

impl<D: SqlDialect> FunctionArgs<D> for () {
    fn render(&mut self, _writer: &mut Writer<D>) -> Result<(), Error> {
        Ok(())
    }
}

impl<D: SqlDialect, T> FunctionArgs<D> for Expr<T, D> {
    fn render(&mut self, writer: &mut Writer<D>) -> Result<(), Error> {
        self.render(writer)
    }
}

impl<D: SqlDialect> FunctionArgs<D> for ColumnRef {
    fn render(&mut self, writer: &mut Writer<D>) -> Result<(), Error> {
        render_column_ref(self, writer);
        Ok(())
    }
}

macro_rules! impl_function_args_tuple {
    ($($ty:ident : $index:tt),+ $(,)?) => {
        impl<D: SqlDialect, $($ty),+> FunctionArgs<D> for ($($ty,)+)
        where
            $($ty: FunctionArgs<D>,)+
        {
            fn render(&mut self, writer: &mut Writer<D>) -> Result<(), Error> {
                let mut first = true;
                $(
                    if !first {
                        writer.write_comma();
                    }
                    first = false;
                    self.$index.render(writer)?;
                )+
                Ok(())
            }
        }
    };
}

impl<D: SqlDialect, A> FunctionArgs<D> for (A,)
where
    A: FunctionArgs<D>,
{
    fn render(&mut self, writer: &mut Writer<D>) -> Result<(), Error> {
        self.0.render(writer)
    }
}

impl_function_args_tuple!(A: 0, B: 1);
impl_function_args_tuple!(A: 0, B: 1, C: 2);
impl_function_args_tuple!(A: 0, B: 1, C: 2, E: 3);
impl_function_args_tuple!(A: 0, B: 1, C: 2, E: 3, F: 4);
impl_function_args_tuple!(A: 0, B: 1, C: 2, E: 3, F: 4, G: 5);
impl_function_args_tuple!(A: 0, B: 1, C: 2, E: 3, F: 4, G: 5, H: 6);
impl_function_args_tuple!(A: 0, B: 1, C: 2, E: 3, F: 4, G: 5, H: 6, I: 7);
impl_function_args_tuple!(A: 0, B: 1, C: 2, E: 3, F: 4, G: 5, H: 6, I: 7, J: 8);
impl_function_args_tuple!(A: 0, B: 1, C: 2, E: 3, F: 4, G: 5, H: 6, I: 7, J: 8, K: 9);
impl_function_args_tuple!(A: 0, B: 1, C: 2, E: 3, F: 4, G: 5, H: 6, I: 7, J: 8, K: 9, L: 10);
impl_function_args_tuple!(A: 0, B: 1, C: 2, E: 3, F: 4, G: 5, H: 6, I: 7, J: 8, K: 9, L: 10, M: 11);
impl_function_args_tuple!(A: 0, B: 1, C: 2, E: 3, F: 4, G: 5, H: 6, I: 7, J: 8, K: 9, L: 10, M: 11, N: 12);
impl_function_args_tuple!(A: 0, B: 1, C: 2, E: 3, F: 4, G: 5, H: 6, I: 7, J: 8, K: 9, L: 10, M: 11, N: 12, O: 13);
impl_function_args_tuple!(A: 0, B: 1, C: 2, E: 3, F: 4, G: 5, H: 6, I: 7, J: 8, K: 9, L: 10, M: 11, N: 12, O: 13, P: 14);
impl_function_args_tuple!(A: 0, B: 1, C: 2, E: 3, F: 4, G: 5, H: 6, I: 7, J: 8, K: 9, L: 10, M: 11, N: 12, O: 13, P: 14, Q: 15);

/// A SQL function that can be rendered for a specific dialect.
pub trait Function<D: SqlDialect> {
    type Args: FunctionArgs<D>;
    type Output;
    const NAME: &'static str;

    fn render(args: &mut Self::Args, writer: &mut Writer<D>) -> Result<(), Error> {
        writer.push(Self::NAME);
        writer.write_group_start();
        args.render(writer)?;
        writer.write_group_end();
        Ok(())
    }
}

/// Calls a typed SQL function.
pub fn call<D: SqlDialect, F: Function<D> + 'static>(args: F::Args) -> Expr<F::Output, D> {
    Expr::new(FunctionExpr::<D, F> {
        args,
        _marker: PhantomData,
    })
}

struct FunctionExpr<D: SqlDialect, F: Function<D>> {
    args: F::Args,
    _marker: PhantomData<fn() -> (D, F)>,
}

impl<D, F> CustomNode<D> for FunctionExpr<D, F>
where
    D: SqlDialect,
    F: Function<D>,
{
    fn render(&mut self, writer: &mut Writer<D>) -> Result<(), Error> {
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
pub trait Operator<D: SqlDialect> {
    type Left: FunctionArgs<D>;
    type Right: FunctionArgs<D>;
    type Output;
    const SQL: &'static str;
    const FIXITY: Fixity;
}

/// A custom operator expression.
pub struct OperatorExpr<O: Operator<D>, D: SqlDialect> {
    pub left: O::Left,
    pub right: O::Right,
    _marker: PhantomData<fn() -> (O, D)>,
}

impl<O: Operator<D>, D: SqlDialect> OperatorExpr<O, D> {
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

impl<O, D> CustomNode<D> for OperatorExpr<O, D>
where
    O: Operator<D>,
    D: SqlDialect,
{
    fn render(&mut self, writer: &mut Writer<D>) -> Result<(), Error> {
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
                writer.push(" ");
                writer.write_group_start();
                self.right.render(writer)?;
                writer.write_group_end();
                Ok(())
            }
            Fixity::Postfix => {
                writer.write_group_start();
                self.left.render(writer)?;
                writer.write_group_end();
                writer.push(" ");
                writer.push(O::SQL);
                Ok(())
            }
        }
    }
}
