use std::marker::PhantomData;

use crate::plan::backend::Dialect;

use super::{Expr, ExprKind, IntoExpr};

/// A `CASE` expression under construction.
///
/// Start one with [`Expr::case`], add branches with [`Case::when`], and finish
/// with either [`Case::otherwise`] or [`Case::end`].
pub struct Case<T, D: Dialect> {
    pub(super) branches: Vec<(Expr<(), D>, Expr<(), D>)>,
    pub(super) _marker: PhantomData<fn() -> T>,
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
