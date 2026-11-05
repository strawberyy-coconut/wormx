//! SQL syntax, expressions, and renderers.
//!
//! This is the SQL interpretation of the plans in [`crate::plan`]. A backend
//! for another query language is expected to provide its own writer and
//! renderers rather than implement these traits.
//!
//! The module is split into:
//!
//! - [`syntax`]: `Dialect`, `SqlSyntax`, and capability markers.
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
