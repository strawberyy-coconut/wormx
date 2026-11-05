//! Core runtime, schema metadata, query plans, SQL rendering, and execution
//! for wormx.
//!
//! The crate is split into four layers:
//!
//! - [`schema`]: backend-neutral table and relation metadata.
//! - [`plan`]: typed builders and the plans they produce.
//! - [`sql`]: SQL syntax, expressions, and renderers.
//! - [`exec`]: database handles, raw queries, and transactions.
//!
//! The root re-exports the same names as before, and the old module paths
//! (`query`, `dml`, `join`, `dialect`, `metadata`, …) remain available as
//! aliases for compatibility.

pub mod config;
pub mod exec;
pub mod plan;
pub mod schema;
pub mod sql;

pub use config::{
    CodegenConfig, Config, ConfigError, PreferredDateTime, PreferredNumeric, TableConfig,
};
pub use exec::{Db, RawQuery, Transaction};
pub use plan::deferred::Deferred;
pub use plan::dml::{Delete, Insert, Update, Upsert};
pub use plan::include::Include;
pub use plan::join::{Join, JoinKind, JoinSource, TableSource};
pub use plan::query::{
    Column, ColumnSelect, CountPlan, CountProjection, Cte, DeletePlan, DeleteRenderer, Filtered,
    Get, InsertPlan, InsertRenderer, IntoSelect, ProjectedSelect, Projection, ProjectionWriter,
    Query, Scalar, Select, SelectPlan, SelectRenderer, StarProjection, Subquery, UpdatePlan,
    UpdateRenderer, UpsertPlan, UpsertRenderer,
};
pub use plan::relation::{BelongsTo, BelongsToRef, HasMany, HasManyRef, OptionalBelongsToRef};
pub use schema::{ColumnDef, RelationDef, RelationKind, Table, TableDef, TableRef, TableRefError};
pub use sql::render::{
    SqlDelete, SqlInsert, SqlSelect, SqlUpdate, SqlUpsert, write_filters, write_from_filters,
    write_groups_having, write_orders, write_tail,
};
pub use sql::{
    BindExpr, ColumnExpr, Dialect, Expr, Fixity, Function, FunctionArgs, IntoExpr, Operator,
    OperatorExpr, RenderExpr, SqlSyntax, SqlWriter, SupportsArrays, SupportsFullJoin,
    SupportsFullTextSearch, SupportsJson, SupportsReturning, SupportsUpsert, Textual, Window, call,
    coalesce, count_star, dense_rank, rank, row_number,
};

// Backwards-compatible module paths. Prefer `schema`, `plan`, `sql`, and
// `exec` in new code.
pub use exec::db;
pub use plan::{deferred, dml, include, join, query, relation};
pub use schema::{metadata, naming};
pub use sql::dialect;
