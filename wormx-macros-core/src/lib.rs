//! Schema fetching, caching, type mapping, and code generation for wormx's `table!` macro.

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
