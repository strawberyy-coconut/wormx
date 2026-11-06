//! Typed `INSERT`, `UPDATE`, `DELETE`, and `UPSERT` builders.
//!
//! Each builder renders its command through the dialect's operation renderer
//! and executes it on a SQLx pool or transaction. The `*Many` builders group
//! rows into a single statement.

mod batch;
mod delete;
mod insert;
mod insert_many;
mod update;
mod update_many;
mod upsert;
mod upsert_many;

pub use batch::RowBuilder;
pub use delete::Delete;
pub use insert::Insert;
pub use insert_many::InsertMany;
pub use update::Update;
pub use update_many::{UpdateMany, UpdateManyStart, UpdateRow};
pub use upsert::Upsert;
pub use upsert_many::UpsertMany;
