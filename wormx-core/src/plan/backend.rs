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
/// This gates the `returning` methods on the DML builders, so a dialect
/// without `RETURNING` rejects them at compile time.
pub trait SupportsReturning: Dialect {}
/// Marks a dialect that supports `FULL [OUTER] JOIN`.
///
/// This gates [`crate::plan::join::Join::full`] at compile time.
pub trait SupportsFullJoin: Dialect {}
/// Marks a dialect that supports `JOIN ... USING (...)`.
///
/// This gates [`crate::plan::join::Join::using`] at compile time.
pub trait SupportsUsing: Dialect {}
/// Marks a dialect that supports `NATURAL JOIN`.
///
/// This gates [`crate::plan::join::Join::natural`] at compile time.
pub trait SupportsNaturalJoin: Dialect {}
/// Marks a dialect that interprets an opaque source suffix.
///
/// This gates the typed `with_source_suffix` builders. The lower-level
/// [`crate::plan::join::TableSource::with_suffix`] builder remains available as
/// a plan escape hatch.
pub trait SupportsSourceSuffix: Dialect {}
/// Marks a dialect that interprets an opaque statement tail.
///
/// This gates the typed `with_tail` builders.
pub trait SupportsStatementTail: Dialect {}
/// Marks a dialect that supports full-text search.
///
/// Core has no portable full-text query API; this marker is for driver or
/// extension crates that provide one.
pub trait SupportsFullTextSearch: Dialect {}
/// Marks a dialect that supports JSON values.
///
/// Core has no portable JSON query API; this marker is for driver or extension
/// crates that provide one.
pub trait SupportsJson: Dialect {}
/// Marks a dialect that supports array values.
///
/// Core has no portable array query API; this marker is for driver or extension
/// crates that provide one.
pub trait SupportsArrays: Dialect {}
/// Marks a dialect that supports upserts.
///
/// This gates [`crate::exec::db::Db::upsert`] and
/// [`crate::exec::db::Transaction::upsert`].
pub trait SupportsUpsert: Dialect {}
