//! Backend-neutral schema metadata.
//!
//! Nothing in this module depends on SQL, SQLx, or a query language. It is the
//! shared vocabulary that plans, renderers, and introspection all build on.

pub mod metadata;
pub mod naming;

pub use metadata::*;
pub use naming::*;
