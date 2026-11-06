# wormx-drivers-sqlite

SQLite support for [`wormx`](https://crates.io/crates/wormx), built on
`sqlx-sqlite`.

```toml
[dependencies]
wormx = { version = "0.2", features = ["sqlite"] }
```

## Dialect

`SqliteDialect` uses the generic SQL renderers:

- `SqlSelect`
- `SqlInsert`
- `SqlUpdate`
- `SqlDelete`
- `SqlUpsert` (`INSERT ... ON CONFLICT ... DO UPDATE SET`)

Identifiers are quoted with double quotes.

SQLite requires a `LIMIT` clause when `OFFSET` is present. The dialect renders
offset-only queries as:

```sql
LIMIT -1 OFFSET 10
```

## Capabilities

SQLite implements:

- `SupportsReturning`
- `SupportsFullJoin`
- `SupportsUsing`
- `SupportsNaturalJoin`
- `SupportsUpsert`
- `SupportsFullTextSearch`
- `SupportsJson`

The full-text and JSON markers are extension points; core has no portable query
API for them. SQLite does not implement `SupportsArrays`.

## Compile-time introspection

The driver reads `PRAGMA table_xinfo` and SQLite's foreign-key metadata. It
reports primary keys, defaults, generated columns, and foreign keys.

One constraint: SQLite does not expose a portable identity/autoincrement flag in
this introspection path. An `INTEGER PRIMARY KEY` is reported as a primary key,
not as `ColumnDef::identity`.

Enable the `macros` feature to use `TableDriver` directly; the `wormx` facade
enables it through its `sqlite` feature.

## Features

`chrono`, `time`, `uuid`, `json`, `bigdecimal`, and `rust_decimal` forward the
matching SQLx type support.
