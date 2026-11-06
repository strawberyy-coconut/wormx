# wormx-core

Runtime core for [`wormx`](https://crates.io/crates/wormx): schema metadata,
typed query plans, the expression AST, SQL renderers, and SQLx-backed execution.

Most applications should depend on the `wormx` facade instead. `wormx-core` is
for driver crates, extension crates, and backends that need the runtime without
the proc macro.

```toml
[dependencies]
wormx-core = "0.2"
```

## What is in this crate

| Module | Contents |
|---|---|
| `schema` | `TableDef`, `ColumnDef`, `RelationDef`, naming helpers |
| `plan` | typed builders, plan structs, `Expr`/`ExprKind`, renderer traits, relations, deferred loading |
| `sql` | `SqlDialect`, `SqlSyntax`, `SqlWriter`, and concrete SQL renderers |
| `exec` | `Db`, `Transaction`, and raw SQL queries |

`schema` and `plan` are backend-neutral. `sql` is the SQL interpretation of the
plans. Execution is SQLx-native: `Db<D>` holds `Pool<D::DB>` and values use
SQLx's `Encode` / `Decode` / `Type`.

See [layering and capability policy](https://github.com/strawberyy-coconut/wormx/blob/master/docs/design/layering.md)
for the full rationale.

## The dialect seam

A dialect chooses one renderer type per logical operation:

```rust
pub struct PostgresDialect;

impl wormx_core::SqlSyntax for PostgresDialect {}

impl wormx_core::Dialect for PostgresDialect {
    type DB = sqlx_postgres::Postgres;

    type Select = wormx_core::SqlSelect;
    type Insert = wormx_core::SqlInsert;
    type Update = wormx_core::SqlUpdate;
    type Delete = wormx_core::SqlDelete;
    type Upsert = wormx_core::SqlUpsert;

    fn quote_ident(&self, name: &str) -> String {
        format!("\"{}\"", name.replace('"', "\"\""))
    }
}
```

A non-SQL backend implements `Dialect` with its own renderer types and can reuse
the plan, relations, includes, transactions, and `FromRow` machinery. SQL
dialects additionally implement `SqlSyntax`; `SqlDialect` is the blanket
specialization.

Renderers receive plan structs, not SQL fragments. `Join` exposes `kind`,
`source`, and `constraint`, and `ExprKind` is public, so a renderer can inspect a
plan instead of emitting `JOIN ... ON ...`.

## Capability markers

Capabilities are marker traits so unsupported use is a compile-time error:

| Marker | Gates |
|---|---|
| `SupportsReturning` | `.returning()` on DML builders |
| `SupportsFullJoin` | `Join::full` |
| `SupportsUsing` | `Join::using` |
| `SupportsNaturalJoin` | `Join::natural` |
| `SupportsSourceSuffix` | typed `with_source_suffix` builders |
| `SupportsStatementTail` | typed `with_tail` builders |
| `SupportsUpsert` | `Db::upsert` and `Transaction::upsert` |
| `SupportsFullTextSearch` | driver/extension APIs for full-text search |
| `SupportsJson` | driver/extension APIs for JSON values |
| `SupportsArrays` | driver/extension APIs for array values |

Core has no portable query API for full-text search, JSON, or arrays. Those
markers exist for driver and extension crates.

## Query and write surface

`wormx-core` owns the values used by the facade:

```rust
use wormx_core::{Db, Query};

let db = Db::new(pool, dialect);

let users = db
    .query(Query::<Users>::new())
    .filter(Users::fields().active().eq(true))
    .order_by(Users::fields().name().asc())
    .limit(20)
    .exec()
    .await?;
```

The generated API comes from `wormx::table!`; the runtime types here are what the
generated code and application code share. Through the facade, the same query is
usually written as `db.query(Users::all())`. DML builders, batch builders,
transactions, CTEs, joins, window functions, and raw queries live in this crate.

## Extending expressions

Application-level functions and operators can be declared without changing
core. Dialect-specific rendering can implement `Function` or `Operator` with a
marker type supplied by the driver or application:

```rust
use wormx_core::{Expr, Function};
use wormx_drivers_postgres::PostgresDialect;

pub struct Contains;

impl Function<PostgresDialect> for Contains {
    type Args = (Expr<String, PostgresDialect>, Expr<String, PostgresDialect>);
    type Output = bool;
    const NAME: &'static str = "CONTAINS";
}
```

See [backends and renderers](https://github.com/strawberyy-coconut/wormx/blob/master/docs/design/backends.md)
for the renderer extension points and a full example.

## Constraints

- Core models SQL-standard operations, but does not own vendor spelling.
- Vendor syntax belongs in a driver crate, a `SqlSyntax` hook, `sql_function!`
  / `operator!`, or a typed driver extension.
- A standard operation is gated by a capability marker when at least one bundled
  backend cannot render it.
- Opaque source suffixes and statement tails are escape hatches, not core
  features.
- The neutral plan contains no vendor vocabulary.
- SQLx owns execution; there is no separate executor layer.

[SQL fidelity](https://github.com/strawberyy-coconut/wormx/blob/master/docs/design/sql-fidelity.md)
lists the exact supported operations and current non-goals.
