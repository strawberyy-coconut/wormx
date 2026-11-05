//! Typed `INSERT`, `UPDATE`, `DELETE`, and `UPSERT` builders.
//!
//! Each builder renders its command through the dialect's operation renderer
//! and executes it on a SQLx pool or transaction.

mod delete;
mod insert;
mod update;
mod upsert;

pub use delete::Delete;
pub use insert::Insert;
pub use update::Update;
pub use upsert::Upsert;
