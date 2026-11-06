//! SQL syntax, expressions, and renderers.
//!
//! This is the SQL interpretation of the plans in [`crate::plan`]. A backend
//! for another query language is expected to provide its own writer and
//! renderers rather than implement these traits.
//!
//! The module is split into:
//!
//! - [`syntax`]: `SqlSyntax` spelling hooks and SQL-specific marker types.
//! - [`writer`]: the `String` plus bound-arguments writer.
//! - [`expr`]: typed expressions, functions, and operators.
//! - [`render`]: the SQL implementations of the operation renderers.

pub mod dialect;
pub mod expr;
pub mod render;
pub mod syntax;
pub mod writer;

pub use dialect::*;
pub use render::*;

// Backend traits now live in `plan::backend`; re-export them so
// `wormx_core::sql::Dialect` and friends keep working.
pub use crate::plan::backend::*;

// The expression AST is backend-neutral and now lives in `plan::expr`. These
// re-exports keep `wormx_core::sql::Expr` and friends working.
pub use crate::plan::expr::*;
