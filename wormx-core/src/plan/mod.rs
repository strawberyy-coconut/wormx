//! Typed query builders, plans, and relations.
//!
//! Plans describe *what* to do. They do not own SQL spelling; the renderer
//! traits in [`crate::plan::query`] let [`crate::sql`] supply it. The expression type
//! ([`crate::sql::Expr`]) is currently SQL-shaped and renders through
//! [`crate::sql::SqlWriter`], so this layer is reusable by SQL backends. A
//! non-SQL backend is expected to bring its own builders, writer, and
//! renderers while reusing [`crate::schema`].
//!
//! [`crate::exec`] supplies the database handle.

pub mod deferred;
pub mod dml;
pub mod include;
pub mod join;
pub mod query;
pub mod relation;

pub use deferred::*;
pub use dml::*;
pub use include::*;
pub use join::*;
pub use query::*;
pub use relation::*;
