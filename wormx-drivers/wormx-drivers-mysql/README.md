# wormx-drivers-mysql

MySQL support for [`wormx`](https://crates.io/crates/wormx), built on
`sqlx-mysql`.

```toml
[dependencies]
wormx = { version = "0.2", features = ["mysql"] }
```

## Dialect

`MySqlDialect` uses the generic SQL renderers except for upsert, which is
dialect-specific:

- `SqlSelect`
- `SqlInsert`
- `SqlUpdate`
- `SqlDelete`
- `MySqlUpsert` (`INSERT ... ON DUPLICATE KEY UPDATE`)

Identifiers are quoted with backticks.

MySQL requires a `LIMIT` clause when `OFFSET` is present. The dialect renders
offset-only queries with the conventional:

```sql
LIMIT 18446744073709551615 OFFSET 10
```

## Capabilities

MySQL implements:

- `SupportsUsing`
- `SupportsNaturalJoin`
- `SupportsUpsert`
- `SupportsFullTextSearch`
- `SupportsJson`

MySQL does not implement `SupportsReturning`, `SupportsFullJoin`, or
`SupportsArrays`.

## Upsert behavior

MySQL chooses the duplicate key index itself, so `conflict_on(...)` is ignored.
`update_from_input(column)` renders `VALUES(column)`.

```rust
db.upsert::<Users>()
    .value(Users::fields().id(), user.id)
    .value(Users::fields().name(), "Alice".to_owned())
    .update_from_input(Users::fields().name())
    .exec()
    .await?;
```

An upsert must have at least one update assignment.

## Compile-time introspection

The driver reads `information_schema` for defaults, `AUTO_INCREMENT` identity,
generated columns, primary keys, and foreign keys.

Enable the `macros` feature to use `TableDriver` directly; the `wormx` facade
enables it through its `mysql` feature.

## Features

`chrono`, `time`, `uuid`, `json`, `bigdecimal`, and `rust_decimal` forward the
matching SQLx type support.
