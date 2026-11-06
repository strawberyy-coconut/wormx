# wormx-drivers

Built-in database drivers for [`wormx`](https://crates.io/crates/wormx).

Each driver is a separate crate:

- [`wormx-drivers-postgres`](https://crates.io/crates/wormx-drivers-postgres)
- [`wormx-drivers-sqlite`](https://crates.io/crates/wormx-drivers-sqlite)
- [`wormx-drivers-mysql`](https://crates.io/crates/wormx-drivers-mysql)
- [`wormx-drivers-mssql`](https://crates.io/crates/wormx-drivers-mssql)

This umbrella crate re-exports them behind features:

```toml
[dependencies]
wormx-drivers = { version = "0.2", features = ["postgres"] }
```

Enable `macros` on an individual driver when using `table!` introspection
directly. The `wormx` facade wires that up through its own driver features.

## Capability matrix

| Capability | PostgreSQL | SQLite | MySQL | SQL Server |
|---|---:|---:|---:|---:|
| `RETURNING` / `OUTPUT` | yes | yes | no | yes |
| `FULL JOIN` | yes | yes | no | yes |
| `JOIN ... USING` | yes | yes | yes | no |
| `NATURAL JOIN` | yes | yes | yes | no |
| Upsert | yes | yes | yes | yes |
| Source suffix / statement tail | no | no | no | yes |
| Full-text marker | yes | yes | yes | yes |
| JSON marker | yes | yes | yes | no |
| Arrays marker | yes | no | no | no |

"Marker" means the driver implements the capability trait for extension crates;
core does not provide a portable full-text, JSON, or array query API.

## Driver-specific behavior

- [PostgreSQL](https://github.com/strawberyy-coconut/wormx/blob/master/wormx-drivers/wormx-drivers-postgres/README.md):
  `ON CONFLICT`, `RETURNING`, arrays/JSON.
- [SQLite](https://github.com/strawberyy-coconut/wormx/blob/master/wormx-drivers/wormx-drivers-sqlite/README.md):
  `ON CONFLICT`, `RETURNING`, `LIMIT -1 OFFSET` for offset-only queries.
- [MySQL](https://github.com/strawberyy-coconut/wormx/blob/master/wormx-drivers/wormx-drivers-mysql/README.md):
  `ON DUPLICATE KEY UPDATE`, `VALUES(column)` for incoming upsert values.
- [MSSQL](https://github.com/strawberyy-coconut/wormx/blob/master/wormx-drivers/wormx-drivers-mssql/README.md):
  `MERGE ... WITH (HOLDLOCK)`, `OUTPUT`, hints, and `OPTION (...)` tails.

## Feature flags

Driver type features (`chrono`, `time`, `uuid`, `json`, `bigdecimal`,
`rust_decimal`) are forwarded to the matching SQLx driver when that driver is
enabled.

The `json` feature on `wormx-drivers-mssql` is currently a no-op because
`sqlx-mssql-rs` 0.1.x enables `serde_json` but not `serde`.

## Adding a driver

A driver implements SQLx's database traits, wormx's `Dialect`, its operation
renderers, and optionally `SqlSyntax`. Compile-time introspection is supplied by
a `TableDriver` from `wormx-macros-core`.

See [backends and renderers](https://github.com/strawberyy-coconut/wormx/blob/master/docs/design/backends.md)
and [compile-time introspection](https://github.com/strawberyy-coconut/wormx/blob/master/docs/design/compile-time-introspection.md).
