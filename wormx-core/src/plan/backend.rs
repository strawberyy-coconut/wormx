//! Backend and capability traits.
//!
//! A [`Dialect`] is a SQLx-backed query language. `SqlDialect` is the SQL
//! specialization and lives in [`crate::sql`].

use sqlx_core::database::Database;

use super::query::{
    DeleteRenderer, InsertRenderer, SelectRenderer, UpdateRenderer, UpsertRenderer,
};

/// A SQLx-backed query language.
///
/// Implement its renderer associated types with the backend's own renderers.
/// The SQL renderers in [`crate::sql`] are one set of implementations; Cypher
/// or document backends supply their own.
pub trait Dialect: Clone + Send + Sync + 'static {
    type DB: Database;

    /// The renderer used for `SELECT` operations.
    type Select: SelectRenderer<Self>;
    /// The renderer used for `INSERT` operations.
    type Insert: InsertRenderer<Self>;
    /// The renderer used for `UPDATE` operations.
    type Update: UpdateRenderer<Self>;
    /// The renderer used for `DELETE` operations.
    type Delete: DeleteRenderer<Self>;
    /// The renderer used for `UPSERT` operations.
    type Upsert: UpsertRenderer<Self>;

    /// Quotes an identifier in the query language.
    fn quote_ident(&self, name: &str) -> String;
}

/// Marks a dialect that supports `RETURNING`.
///
/// This gates the `exec_returning` methods on the DML builders, so a dialect
/// without `RETURNING` rejects them at compile time.
pub trait SupportsReturning: Dialect {}
/// Marks a dialect that supports `FULL [OUTER] JOIN`.
///
/// This gates [`crate::plan::join::Join::full`] at compile time.
pub trait SupportsFullJoin: Dialect {}
/// Marks a dialect that supports full-text search.
pub trait SupportsFullTextSearch: Dialect {}
/// Marks a dialect that supports JSON values.
pub trait SupportsJson: Dialect {}
/// Marks a dialect that supports array values.
pub trait SupportsArrays: Dialect {}
/// Marks a dialect that supports upserts.
///
/// This gates [`crate::exec::db::Db::upsert`] and
/// [`crate::exec::db::Transaction::upsert`].
pub trait SupportsUpsert: Dialect {}
