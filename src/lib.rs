//! Typed table generation and query building for sqlx.
//!
//! wormx provides compile-time table introspection and a typed query DSL over
//! SQLx pools. `wormx::table!` reads the migrated schema and generates row
//! types, table metadata, and relation accessors. Use SQLx migrations
//! (`sqlx migrate`, `sqlx-cli`, or `sqlx::migrate!`) to create and evolve the
//! schema before building.

pub use sqlx;
pub use wormx_core::{
    BelongsTo, BelongsToRef, BindValue, Column, ColumnDef, ColumnExpr, ColumnRef, Config,
    ConfigError, CountPlan, CountProjection, Cte, CustomNode, Db, Deferred, Delete, DeletePlan,
    DeleteRenderer, Dialect, Expr, ExprKind, Filtered, Fixity, Function, FunctionArgs, Get,
    HasMany, HasManyRef, Include, Insert, InsertPlan, InsertRenderer, IntoExpr, IntoSelect, Join,
    JoinKind, JoinSource, LogicalOp, Operator, OperatorExpr, OptionalBelongsToRef, Order,
    ProjectedSelect, Projection, ProjectionVisitor, Query, RawQuery, RelationDef, RelationKind,
    Scalar, Select, SelectPlan, SelectRenderer, SqlDelete, SqlDialect, SqlInsert, SqlSelect,
    SqlSyntax, SqlUpdate, SqlUpsert, SqlWriter, StarProjection, Subquery, SupportsArrays,
    SupportsFullJoin, SupportsFullTextSearch, SupportsJson, SupportsReturning, SupportsUpsert,
    Table, TableDef, TableRef, TableRefError, TableSource, Textual, Transaction, Update,
    UpdatePlan, UpdateRenderer, Upsert, UpsertPlan, UpsertRenderer, Window, Writer, call, coalesce,
    count_star, dense_rank, rank, row_number, write_ctes, write_filters, write_from_filters,
    write_groups_having, write_orders, write_projection, write_tail,
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
///
/// # Contract and caveats
///
/// - The return type is a declaration, not a check. Nothing validates the
///   actual SQL function's result type, so `-> i32` compiles even when the
///   database function returns text.
/// - Without `sql = "..."`, the SQL name is `stringify!($name)`. Use the
///   `sql =` form for names such as `COUNT_BIG`, `CONTAINS`, schema-qualified
///   names, or reserved words.
/// - Arguments are erased to `Expr<(), D>` after conversion. The macro checks
///   arity but not each argument's SQL type.
/// - The generated function is generic over `D: Dialect`. A standalone call
///   needs a dialect in scope or a turbofish; inside `db.query(...)` it is
///   usually inferred. If inference fails, the error is typically
///   “type annotations needed”.
/// - Zero-argument functions are accepted, for example
///   `wormx::sql_function! { fn now() -> String; }`.
/// - For dialect-specific rendering, implement [`Function`] with a marker type
///   instead. `sql_function!` fixes one spelling for every dialect.
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

/// Declares a typed SQL operator.
///
/// ```ignore
/// wormx::operator! {
///     pub fn concat(left: String, right: String) -> String = "||";
///     fixity = Infix;
///     grouping = Operands; // Operands | None | Precedence(6)
/// }
///
/// wormx::operator! {
///     pub fn is_unknown(value: String) -> bool = "IS UNKNOWN";
///     fixity = Postfix;
///     grouping = Operands;
/// }
/// ```
///
/// `grouping = Operands` (the default) parenthesizes each operand, so nested
/// logical expressions keep their intended association. `grouping = None` opts
/// out and is only safe when the operator's SQL grammar makes grouping
/// unnecessary. `Precedence(n)` is accepted but currently renders like
/// `Operands`; a future release may use the declared precedence to drop
/// redundant parentheses.
///
/// Caveats:
///
/// - The return type is a declaration, not a check, and operands are type-erased
///   after `IntoExpr`. The macro checks arity but not SQL result types.
/// - The SQL spelling is a single `&'static str` used for every dialect;
///   implement [`Operator`] for a concrete dialect when the spelling differs.
/// - `Column<T>` carries a table-name string, not a table type, so columns from
///   different tables can be composed without a compile error.
/// - There is no associativity declaration; `a OP b OP c` follows the SQL
///   database's precedence rules.
/// - Only unary and binary shapes are supported. `BETWEEN`, `CASE`, and other
///   multi-token forms need [`Operator`] or [`Function`].
/// - Prefix and postfix operators always write a space around the operator.
///   That is valid for words such as `NOT`, but glued operators render as
///   `x :: int` rather than `x::int`.
/// - The generated name is a free function in the caller's crate, so it can
///   collide with generated `fields()` accessors or other helpers.
/// - For negation prefer the built-in `.not()`, which has the same safe
///   grouping.
#[macro_export]
macro_rules! operator {
    // Infix with explicit grouping.
    (
        $vis:vis fn $name:ident($left:ident : $lty:ty, $right:ident : $rty:ty) -> $ret:ty = $sql:expr;
        fixity = Infix;
        grouping = Operands;
    ) => {
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
    (
        $vis:vis fn $name:ident($left:ident : $lty:ty, $right:ident : $rty:ty) -> $ret:ty = $sql:expr;
        fixity = Infix;
        grouping = None;
    ) => {
        $vis fn $name<D: $crate::Dialect, L, R>(left: L, right: R) -> $crate::Expr<$ret, D>
        where
            L: $crate::IntoExpr<$lty, D>,
            R: $crate::IntoExpr<$rty, D>,
        {
            $crate::Expr::<$ret, D>::infix_ungrouped(
                $crate::IntoExpr::<$lty, D>::into_expr(left),
                $sql,
                $crate::IntoExpr::<$rty, D>::into_expr(right),
            )
        }
    };
    (
        $vis:vis fn $name:ident($left:ident : $lty:ty, $right:ident : $rty:ty) -> $ret:ty = $sql:expr;
        fixity = Infix;
        grouping = Precedence($prec:literal);
    ) => {
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
    // Prefix.
    (
        $vis:vis fn $name:ident($operand:ident : $ty:ty) -> $ret:ty = $sql:expr;
        fixity = Prefix;
        grouping = Operands;
    ) => {
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
    (
        $vis:vis fn $name:ident($operand:ident : $ty:ty) -> $ret:ty = $sql:expr;
        fixity = Prefix;
        grouping = None;
    ) => {
        $vis fn $name<D: $crate::Dialect, T>(operand: T) -> $crate::Expr<$ret, D>
        where
            T: $crate::IntoExpr<$ty, D>,
        {
            $crate::Expr::<$ret, D>::prefix_ungrouped(
                $sql,
                $crate::IntoExpr::<$ty, D>::into_expr(operand).erase(),
            )
        }
    };
    (
        $vis:vis fn $name:ident($operand:ident : $ty:ty) -> $ret:ty = $sql:expr;
        fixity = Prefix;
        grouping = Precedence($prec:literal);
    ) => {
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
    // Postfix.
    (
        $vis:vis fn $name:ident($operand:ident : $ty:ty) -> $ret:ty = $sql:expr;
        fixity = Postfix;
        grouping = Operands;
    ) => {
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
    (
        $vis:vis fn $name:ident($operand:ident : $ty:ty) -> $ret:ty = $sql:expr;
        fixity = Postfix;
        grouping = None;
    ) => {
        $vis fn $name<D: $crate::Dialect, T>(operand: T) -> $crate::Expr<$ret, D>
        where
            T: $crate::IntoExpr<$ty, D>,
        {
            $crate::Expr::<$ret, D>::postfix_ungrouped(
                $crate::IntoExpr::<$ty, D>::into_expr(operand).erase(),
                $sql,
            )
        }
    };
    (
        $vis:vis fn $name:ident($operand:ident : $ty:ty) -> $ret:ty = $sql:expr;
        fixity = Postfix;
        grouping = Precedence($prec:literal);
    ) => {
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
    // Default: grouped infix.
    (
        $vis:vis fn $name:ident($left:ident : $lty:ty, $right:ident : $rty:ty) -> $ret:ty = $sql:expr;
    ) => {
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
