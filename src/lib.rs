//! Typed table generation and query building for sqlx.
//!
//! wormx sits on top of SQLx: SQLx owns connections, execution, transactions,
//! and migrations. wormx owns compile-time table introspection and the typed
//! query DSL. Use `sqlx migrate` / `sqlx-cli` (or embedded SQLx migrations) to
//! create and evolve the schema before building.

pub use sqlx;
pub use wormx_core::{
    BelongsTo, BelongsToRef, BindExpr, Capabilities, Column, ColumnDef, ColumnExpr, Config,
    ConfigError, CountPlan, CountProjection, Cte, Db, Deferred, Delete, DeletePlan, DeleteRenderer,
    Dialect, Expr, Filtered, Fixity, Function, FunctionArgs, Get, HasMany, HasManyRef, Include,
    Insert, InsertPlan, InsertRenderer, IntoExpr, IntoSelect, Join, JoinKind, Operator,
    OperatorExpr, OptionalBelongsToRef, ProjectedSelect, Projection, ProjectionWriter, Query,
    RawQuery, RelationDef, RelationKind, RenderExpr, Scalar, Select, SelectPlan, SelectRenderer,
    SqlDelete, SqlInsert, SqlSelect, SqlUpdate, SqlUpsert, SqlWriter, StarProjection, Subquery,
    SupportsArrays, SupportsFullTextSearch, SupportsJson, SupportsReturning, SupportsUpsert, Table,
    TableDef, TableRef, TableRefError, TableSource, Textual, Transaction, Update, UpdatePlan,
    UpdateRenderer, Upsert, UpsertPlan, UpsertRenderer, Window, call, coalesce, count_star,
    dense_rank, rank, row_number,
};

#[cfg(any(
    feature = "postgres",
    feature = "sqlite",
    feature = "mysql",
    feature = "mssql"
))]
pub use wormx_drivers::*;

#[cfg(feature = "macros")]
pub use wormx_macros::table;

#[doc(hidden)]
pub mod __private {
    pub use ::sqlx;
    pub use ::sqlx::{ColumnIndex, Decode, Encode, Error, FromRow, Row, Type};
}

/// Declares a typed SQL function that can be used inside wormx expressions.
///
/// ```ignore
/// wormx::sql_function! {
///     pub fn lower(value: String) -> String;
/// }
///
/// let expr = lower(Users::fields().name());
/// ```
///
/// Arguments accept either expressions (`Column<T>`, `Expr<T, D>`) or Rust values that implement
/// `Encode` and `Type` for the selected database. Use `sql = "..."` when the SQL spelling differs
/// from the Rust function name.
#[macro_export]
macro_rules! sql_function {
    ($vis:vis fn $name:ident($($arg:ident : $ty:ty),* $(,)?) -> $ret:ty; sql = $sql:expr) => {
        $vis fn $name<D: $crate::Dialect>(
            $($arg: impl $crate::IntoExpr<$ty, D>),*
        ) -> $crate::Expr<$ret, D> {
            $crate::Expr::<$ret, D>::function(
                $sql,
                ::std::vec![
                    $($crate::IntoExpr::<$ty, D>::into_expr($arg).erase()),*
                ],
            )
        }
    };
    ($vis:vis fn $name:ident($($arg:ident : $ty:ty),* $(,)?) -> $ret:ty;) => {
        $crate::sql_function! {
            $vis fn $name($($arg: $ty),*) -> $ret;
            sql = ::std::stringify!($name)
        }
    };
}

/// Declares a typed infix operator.
#[macro_export]
macro_rules! infix_operator {
    ($vis:vis fn $name:ident($left:ident : $lty:ty, $right:ident : $rty:ty) -> $ret:ty = $sql:expr;) => {
        $vis fn $name<D: $crate::Dialect, L, R>(left: L, right: R) -> $crate::Expr<$ret, D>
        where
            L: $crate::IntoExpr<$lty, D>,
            R: $crate::IntoExpr<$rty, D>,
        {
            $crate::Expr::<$ret, D>::infix(
                $crate::IntoExpr::<$lty, D>::into_expr(left),
                $sql,
                $crate::IntoExpr::<$rty, D>::into_expr(right),
            )
        }
    };
}

/// Declares a typed prefix operator.
#[macro_export]
macro_rules! prefix_operator {
    ($vis:vis fn $name:ident($operand:ident : $ty:ty) -> $ret:ty = $sql:expr;) => {
        $vis fn $name<D: $crate::Dialect, T>(operand: T) -> $crate::Expr<$ret, D>
        where
            T: $crate::IntoExpr<$ty, D>,
        {
            $crate::Expr::<$ret, D>::prefix(
                $sql,
                $crate::IntoExpr::<$ty, D>::into_expr(operand).erase(),
            )
        }
    };
}

/// Declares a typed postfix operator.
#[macro_export]
macro_rules! postfix_operator {
    ($vis:vis fn $name:ident($operand:ident : $ty:ty) -> $ret:ty = $sql:expr;) => {
        $vis fn $name<D: $crate::Dialect, T>(operand: T) -> $crate::Expr<$ret, D>
        where
            T: $crate::IntoExpr<$ty, D>,
        {
            $crate::Expr::<$ret, D>::postfix(
                $crate::IntoExpr::<$ty, D>::into_expr(operand).erase(),
                $sql,
            )
        }
    };
}
