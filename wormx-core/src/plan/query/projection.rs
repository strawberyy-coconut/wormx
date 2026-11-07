use sqlx_core::error::Result;
use sqlx_core::from_row::FromRow;

use super::column::Column;
use crate::plan::backend::Dialect;
use crate::plan::expr::{ColumnRef, Expr, ExprKind};

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
