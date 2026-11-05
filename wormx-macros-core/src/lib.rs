//! Schema fetching, caching, type mapping, and code generation for wormx's
//! `table!` macro.
//!
//! # Backend seam
//!
//! Introspection is deliberately pluggable. A backend provides a
//! [`TableDriver`] built from a describe function and a constraints function
//! ([`TableDriver::new_with_fns`]), then passes it to [`expand_table`].
//!
//! The built-in `wormx::table!` macro wires up the bundled drivers, but a
//! third-party backend can depend on this crate directly and run its own
//! `table!` entry point without changing wormx. The schema cache in
//! [`cache`] and the code generator in [`generate`] are shared.

pub mod cache;
pub mod constraints;
pub mod driver;
pub mod env;
pub mod expand;
pub mod generate;
pub mod input;
pub mod mapping;

pub use constraints::{Constraints, ForeignKey, IncomingForeignKey};
pub use driver::{ConstraintsFn, TableDataSource, TableDriver};
pub use expand::expand_table;
pub use input::TableMacroInput;
