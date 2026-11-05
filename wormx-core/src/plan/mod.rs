//! Typed query builders, plans, and relations.
//!
//! Plans describe *what* to do. The expression AST in [`expr`] is
//! backend-neutral: each [`Dialect`] supplies renderers that
//! interpret it into its own query language. SQL's renderers live in
//! [`crate::sql`], but the plan does not depend on SQL spelling.
//!
//! A dialect is still SQLx-backed ([`Dialect::DB`] is a
//! `sqlx::Database`), so rows, arguments, encoding, decoding, and execution
//! stay on SQLx. The query *language* is what the renderers own.
//!
//! [`crate::exec`] supplies the database handle.

pub mod backend;
pub mod command;
pub mod deferred;
pub mod dml;
pub mod expr;
pub mod include;
pub mod join;
pub mod query;
pub mod relation;

pub use backend::*;
pub use command::*;
pub use deferred::*;
pub use dml::*;
pub use expr::*;
pub use include::*;
pub use join::*;
pub use query::*;
pub use relation::*;
