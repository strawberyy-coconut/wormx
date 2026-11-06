# wormx-drivers-mssql

Microsoft SQL Server support for [`wormx`](https://crates.io/crates/wormx),
built on `sqlx-mssql-rs`.

```toml
[dependencies]
wormx = { version = "0.2", features = ["mssql"] }
sqlx-mssql-rs = "0.1"
```

MSSQL connection URLs use `mssql://`:

```text
mssql://sa:Password1!@mssql:1433/master?encrypt=on&trust_certificate=true
```

## Dialect

`MssqlDialect` uses the generic SQL renderers except for upsert:

- `SqlSelect`
- `SqlInsert`
- `SqlUpdate`
- `SqlDelete`
- `MssqlUpsert` (`MERGE ... WITH (HOLDLOCK)`)

Identifiers are quoted with brackets.

## Capabilities

SQL Server implements:

- `SupportsReturning` — `OUTPUT INSERTED.*` / `OUTPUT DELETED.*`
- `SupportsFullJoin`
- `SupportsUpsert`
- `SupportsFullTextSearch`
- `SupportsSourceSuffix` — table hints
- `SupportsStatementTail` — `OPTION (...)`

SQL Server does not implement `SupportsUsing` or `SupportsNaturalJoin`, so joins
must use `ON`. It does not implement `SupportsArrays` or `SupportsJson`.

## Upsert behavior

`MssqlUpsert` emits:

```sql
MERGE INTO target WITH (HOLDLOCK) AS target
USING (...) AS source
ON target.key = source.key
WHEN MATCHED THEN UPDATE SET ...
WHEN NOT MATCHED THEN INSERT (...) VALUES (...);
```

`conflict_on(...)` is the merge join key. `update_from_input(column)` renders
`source.column`. The insert branch omits identity and generated columns, so a
new row receives a generated identity value. `.returning()` emits
`OUTPUT INSERTED.*` after the final `WHEN` clause.

## Table hints and statement options

Core knows only opaque source suffixes and statement tails. This crate adds SQL
Server vocabulary:

```rust
use wormx::{MssqlStatementExt, MssqlTableExt, TableHint};

let rows = db
    .query(Users::all())
    .hint("ROWLOCK, READPAST")
    .option("RECOMPILE")
    .exec()
    .await?;

db.update::<Users>()
    .hint(TableHint::RowLock)
    .set(Users::fields().active(), false)
    .filter(Users::fields().id().eq(1_i64))
    .exec()
    .await?;
```

`TableHint` covers common hints; use `TableHint::Raw` for others or for a
combined string.

## Compile-time introspection

The driver reads `sys.columns`, `sys.indexes`, and related catalog views for
defaults, identity, generated columns, primary keys, and foreign keys.

Enable the `macros` feature to use `TableDriver` directly; the `wormx` facade
enables it through its `mssql` feature.

## Features

`chrono`, `time`, `uuid`, `bigdecimal`, and `rust_decimal` forward the matching
`sqlx-mssql-rs` type support.

The `json` feature is currently a no-op because `sqlx-mssql-rs` 0.1.x enables
`serde_json` but not `serde`, so enabling the driver's JSON support fails to
compile.
