//! Core runtime, SQL dialect traits, and the typed query DSL for wormx.

pub mod config;
pub mod db;
pub mod deferred;
pub mod dialect;
pub mod dml;
pub mod include;
pub mod join;
pub mod metadata;
pub mod naming;
pub mod query;
pub mod relation;

pub use config::{
    CodegenConfig, Config, ConfigError, PreferredDateTime, PreferredNumeric, TableConfig,
};
pub use db::{Db, RawQuery, Transaction};
pub use deferred::Deferred;
pub use dialect::{
    BindExpr, ColumnExpr, Dialect, Expr, Fixity, Function, FunctionArgs, IntoExpr, Operator,
    OperatorExpr, RenderExpr, SqlSyntax, SqlWriter, SupportsArrays, SupportsFullTextSearch,
    SupportsJson, SupportsReturning, SupportsUpsert, Textual, Window, call, coalesce, count_star,
    dense_rank, rank, row_number,
};
pub use dml::{Delete, Insert, Update, Upsert};
pub use include::Include;
pub use join::{Join, JoinKind, TableSource};
pub use metadata::{
    ColumnDef, RelationDef, RelationKind, Table, TableDef, TableRef, TableRefError,
};
pub use query::{
    Column, ColumnSelect, CountPlan, CountProjection, Cte, DeletePlan, DeleteRenderer, Filtered,
    Get, InsertPlan, InsertRenderer, IntoSelect, ProjectedSelect, Projection, ProjectionWriter,
    Query, Scalar, Select, SelectPlan, SelectRenderer, SqlDelete, SqlInsert, SqlSelect, SqlUpdate,
    SqlUpsert, StarProjection, Subquery, UpdatePlan, UpdateRenderer, UpsertPlan, UpsertRenderer,
    write_filters, write_from_filters, write_groups_having, write_orders, write_tail,
};
pub use relation::{BelongsTo, BelongsToRef, HasMany, HasManyRef, OptionalBelongsToRef};
