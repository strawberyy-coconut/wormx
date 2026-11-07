//! Core runtime, schema metadata, query plans, SQL rendering, and execution
//! for wormx.
//!
//! The crate is split into four layers:
//!
//! - [`schema`]: backend-neutral table and relation metadata.
//! - [`plan`]: typed builders, the neutral expression AST, and renderer traits.
//! - [`sql`]: SQL syntax, SQL writer helpers, and SQL renderers.
//! - [`exec`]: database handles, raw queries, and transactions.
//!
//! The root re-exports the same names as before, and the old module paths
//! (`query`, `dml`, `join`, `dialect`, `metadata`, …) remain available as
//! aliases for compatibility.
//!
//! Core stays deliberately lean: SQL-standard operations are expected to be
//! supported, even when one bundled driver needs a capability flag to gate the
//! typed entry point. Dialect-specific spellings belong in a driver, behind a
//! `SqlSyntax` hook, or in an expression built with `sql_function!` or
//! `operator!`.

pub mod config;
pub mod exec;
pub mod plan;
pub mod schema;
pub mod sql;

pub use config::{
    CodegenConfig, Config, ConfigError, PreferredDateTime, PreferredNumeric, RelationMode,
    TableConfig,
};
pub use exec::{Db, RawQuery, Transaction};
pub use plan::deferred::Deferred;
pub use plan::dml::{
    Delete, Insert, InsertMany, Returning, RowBuilder, Update, UpdateMany, UpdateManyStart,
    UpdateRow, Upsert, UpsertMany,
};
pub use plan::expr::{
    BindValue, Case, ColumnExpr, ColumnRef, CustomNode, Expr, ExprKind, IntoExpr, LogicalOp, Order,
    Window, coalesce, count_star, dense_rank, rank, row_number,
};
pub use plan::include::Include;
pub use plan::join::{Join, JoinConstraint, JoinKind, JoinSource, TableSource};
pub use plan::query::{
    Column, ColumnSelect, CountPlan, CountProjection, Cte, DeletePlan, DeleteRenderer, Filtered,
    Get, InsertManyPlan, InsertPlan, InsertRenderer, IntoSelect, ProjectedSelect, Projection,
    ProjectionVisitor, Query, Scalar, Select, SelectPlan, SelectRenderer, StarProjection, Subquery,
    UpdatePlan, UpdateRenderer, UpsertManyPlan, UpsertPlan, UpsertRenderer,
};
pub use plan::relation::{BelongsTo, BelongsToRef, HasMany, HasManyRef, OptionalBelongsToRef};
pub use schema::{ColumnDef, RelationDef, RelationKind, Table, TableDef, TableRef, TableRefError};
pub use sql::render::{
    SqlDelete, SqlInsert, SqlSelect, SqlUpdate, SqlUpsert, write_ctes, write_filters,
    write_from_filters, write_groups_having, write_orders, write_projection, write_tail,
    write_values_rows,
};
pub use sql::{
    Dialect, Fixity, Function, FunctionArgs, Operator, OperatorExpr, ReturningOp,
    ReturningPosition, SqlDialect, SqlSyntax, SqlWriter, SupportsArrays, SupportsFullJoin,
    SupportsFullTextSearch, SupportsJson, SupportsNaturalJoin, SupportsReturning,
    SupportsSourceSuffix, SupportsStatementTail, SupportsUpsert, SupportsUsing, Textual, Writer,
    call,
};

// Backwards-compatible module paths. Prefer `schema`, `plan`, `sql`, and
// `exec` in new code.
pub use exec::db;
pub use plan::{deferred, dml, include, join, query, relation};
pub use schema::{metadata, naming};
pub use sql::dialect;
