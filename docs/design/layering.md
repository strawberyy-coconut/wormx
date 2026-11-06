# Layering and capability policy

`wormx` is deliberately split so that schema and plan logic can be reused by a
backend that does not speak SQL, while SQL-specific spelling stays in one place.

## Crate map

```text
wormx
├── wormx-macros             proc-macro entry point: table!
├── wormx-macros-core        introspection, cache, config, codegen
├── wormx-drivers            umbrella for built-in drivers
│   ├── wormx-drivers-postgres
│   ├── wormx-drivers-sqlite
│   ├── wormx-drivers-mysql
│   └── wormx-drivers-mssql
└── wormx-core               runtime, plans, expressions, SQL renderers, exec
```

`wormx` is the application facade. It re-exports the runtime and the built-in
drivers behind features. Generated code refers to `::wormx`, so `wormx` must be a
direct dependency.

## Runtime layers

`wormx-core` has four layers:

| Layer | Responsibility | SQL-aware? |
|---|---|---|
| `schema` | `TableDef`, `ColumnDef`, `RelationDef` metadata | No |
| `plan` | typed builders, plan structs, `Expr`/`ExprKind`, renderer traits, relations, deferred loading | No |
| `sql` | `SqlDialect`, `SqlSyntax`, `SqlWriter`, and concrete SQL renderers | Yes |
| `exec` | `Db`, `Transaction`, raw queries | SQLx-backed, language-neutral |

`schema` and `plan` are backend-neutral. Plans describe *what* to do; a dialect
supplies renderers that interpret the plan. Execution stays SQLx-native:
`Db<D>` holds `Pool<D::DB>`, and values use SQLx's `Encode`/`Decode`/`Type`.

A dialect is SQLx-backed but not necessarily SQL:

```rust
pub trait Dialect: Clone + Send + Sync + 'static {
    type DB: sqlx::Database;

    type Select: SelectRenderer<Self>;
    type Insert: InsertRenderer<Self>;
    type Update: UpdateRenderer<Self>;
    type Delete: DeleteRenderer<Self>;
    type Upsert: UpsertRenderer<Self>;

    fn quote_ident(&self, name: &str) -> String;
}
```

A Cypher or document backend therefore implements `sqlx::Database`, chooses its
own renderer types, and can reuse the plan, relations, includes, transactions,
and `FromRow` machinery. `SqlSyntax` and `SqlDialect` are the SQL
specialization:

```rust
pub trait SqlDialect: Dialect + SqlSyntax {}

impl<D: Dialect + SqlSyntax> SqlDialect for D {}
```

## Renderers receive plans

Renderers do not receive a partially built SQL string. They receive plan structs,
so they can map a plan to their own constructs:

- `Join` exposes `kind`, `source`, and `constraint`;
- `ExprKind` is public;
- `SelectPlan` and `CountPlan` expose the base table as a `TableSource`.

A custom SQL renderer can reuse the shared clause helpers (`write_from_filters`,
`write_groups_having`, `write_orders`, `write_tail`) plus `Join::render` and
`TableSource::render` instead of copying `SqlSelect`.

## What belongs in core

`wormx-core` is deliberately lean, not deliberately incomplete.

Constraints:

- Core models SQL-standard operations. A standard operation belongs in core even
  when one bundled driver cannot render it.
- When support is not universal, core models the operation and a
  `Supports...` marker gates the typed entry point. Unsupported use is a
  compile-time error instead of a database error.
- Core does not own vendor spelling. Vendor-only syntax belongs in a driver
  crate, behind a `SqlSyntax` hook, in `sql_function!` / `operator!`, or in a
  typed driver extension.
- Opaque source suffixes and statement tails are escape hatches, not core
  features.
- "Lean" means the neutral plan contains no vendor vocabulary. It does not mean
  omitting a standard SQL feature merely because one backend lacks it.

## Capability policy

Capabilities are marker traits:

- Add a marker when a standard or expected operation is missing from at least
  one supported backend.
- Gate the typed constructor or execution method with that marker.
- Keep the plan value inspectable so custom renderers can support more.
- Do not add a marker when every supported backend can render the operation.
- Do not silently ignore an unsupported capability.

Current markers include:

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

`SupportsFullTextSearch`, `SupportsJson`, and `SupportsArrays` are extension
markers: core has no portable query API for those features, but driver crates
can use them to expose one.

## Decisions

- **No separate executor layer.** SQLx owns execution. The dialect layer owns the
  command string; the string is opaque to SQLx, so a backend can interpret it as
  SQL or as its own command language.
- **Associated renderer types instead of specialization.** A dialect chooses a
  renderer type per operation rather than overriding a blanket implementation.
  `SqlSelect`, `SqlInsert`, `SqlUpdate`, `SqlDelete`, and `SqlUpsert` are the
  generic SQL renderers.
- **Stateful plans stay inspectable.** Builders keep plans as values until
  execution, which is what lets a custom renderer understand them.
- **Capabilities are compile-time.** A missing capability should be a type error
  at the call site, not a stringly runtime rejection.

## Future

- Further split the SQL render helpers if third-party SQL backends need a
  narrower seam than `SqlDialect`.
- Decide whether non-SQL backends need a first-class `Writer` type separate from
  the SQL spelling helpers. Today `Writer` is language-neutral and the helpers
  are gated behind `SqlDialect`.
