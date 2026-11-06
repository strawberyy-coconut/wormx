# wormx-drivers-postgres

PostgreSQL support for [`wormx`](https://crates.io/crates/wormx), built on
`sqlx-postgres`.

```toml
[dependencies]
wormx = { version = "0.2", features = ["postgres"] }
```

## Dialect

`PostgresDialect` uses the generic SQL renderers:

- `SqlSelect`
- `SqlInsert`
- `SqlUpdate`
- `SqlDelete`
- `SqlUpsert` (`INSERT ... ON CONFLICT ... DO UPDATE SET`)

Identifiers are quoted with double quotes.

## Capabilities

PostgreSQL implements:

- `SupportsReturning` — `RETURNING` / `.returning().exec()`
- `SupportsFullJoin`
- `SupportsUsing`
- `SupportsNaturalJoin`
- `SupportsUpsert`
- `SupportsFullTextSearch`
- `SupportsJson`
- `SupportsArrays`

The last three are extension markers; core has no portable API for those
features.

## Upsert behavior

`conflict_on(...)` selects the PostgreSQL conflict target. `update_from_input`
renders `excluded.column`.

```rust
db.upsert::<Users>()
    .value(Users::fields().id(), user.id)
    .value(Users::fields().name(), "Alice".to_owned())
    .conflict_on(Users::fields().id())
    .update_from_input(Users::fields().name())
    .exec()
    .await?;
```

## Compile-time introspection

The driver provides schema describe and constraint functions for PostgreSQL
catalog tables, including identity, generated columns, defaults, and foreign
keys. Enable the `macros` feature to use `TableDriver` directly; the `wormx`
facade enables it through its `postgres` feature.

## Features

`chrono`, `time`, `uuid`, `json`, `bigdecimal`, and `rust_decimal` forward the
matching SQLx type support.
