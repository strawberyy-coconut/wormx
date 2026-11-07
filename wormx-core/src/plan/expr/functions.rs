use crate::plan::backend::Dialect;

use super::{Expr, ExprKind, IntoExpr};

/// `COUNT(*)`.
pub fn count_star<D: Dialect>() -> Expr<i64, D> {
    Expr::from_kind(ExprKind::Count {
        operand: Box::new(Expr::from_kind(ExprKind::Star)),
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
