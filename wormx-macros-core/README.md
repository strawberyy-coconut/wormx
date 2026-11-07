# wormx-macros-core

Schema introspection, caching, configuration, and code generation for
[`wormx`](https://crates.io/crates/wormx)'s `table!` macro.

This crate is the reusable backend seam. It is not a proc-macro crate itself;
[`wormx-macros`](https://crates.io/crates/wormx-macros) is the thin `table!`
entry point for the bundled drivers.

```toml
[dependencies]
wormx-macros-core = "0.2"
```

## Pipeline

```text
parse table reference
  -> load wormx.toml / sqlx.toml and environment
  -> look up the schema cache
  -> describe the table and read constraints (on a cache miss)
  -> write the cache
  -> run the shared code generator
```

The cache is keyed by database name, schema, and table name. A cache entry
contains the serialized `Describe` metadata and constraints only — never
credentials or row data.

See [compile-time introspection](https://github.com/strawberyy-coconut/wormx/blob/master/docs/design/compile-time-introspection.md)
for the full pipeline, configuration reference, and cache rules.

## `TableDriver`

A backend adapter implements `TableDriver` with two functions:

- a describe function that returns serialized `Describe` metadata;
- a constraints function that reads primary keys, defaults, identity/generated
  columns, and foreign keys.

```rust
const DRIVER: TableDriver = TableDriver::new_with_fns::<sqlx_mssql_rs::Mssql>(
    &["mssql"], // aliases, no core patch needed
    describe_mssql,
    constraints_mssql,
);

#[proc_macro]
pub fn table(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as TableMacroInput);
    match wormx_macros_core::expand_table(input, &[DRIVER]) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}
```

The built-in `wormx::table!` only wires up the bundled drivers. A third-party
backend can call `expand_table` with its own driver or expose its own `table!`
macro.

## Configuration

`wormx.toml` is read from the crate root. `sqlx.toml` is also read; its
`[common]`, `[macros.preferred-crates]`, `[macros.type-overrides]`, and
`[macros.table-overrides]` settings are defaults, and `wormx.toml` wins on
conflicts.

```toml
[wormx]
cache-dir = ".wormx"

# Force a built-in driver instead of inferring it from the URL scheme.
# dialect = "postgres"

[wormx.database]
url-var = "DATABASE_URL"

[wormx.codegen]
derives = ["Debug", "Clone"]
visibility = "pub"
from-row = true
relations = "auto"

[wormx.tables."public.users"]
struct = "User"
rust-path = "crate::models::User"
skip-columns = ["password_hash"]
relations = "explicit"
include-relations = ["blogs"]
skip-relations = ["audit_logs"]
```

Type and column overrides use the `[macros]` sections from `wormx.toml` or
`sqlx.toml`:

```toml
[macros.preferred-crates]
date-time = "chrono"
numeric = "rust_decimal"

[macros.type-overrides]
"UUID" = "uuid::Uuid"
"public.mood" = "crate::Mood"

[macros.table-overrides."public.users"]
"name" = "crate::UserName"
```

## Cache and offline builds

The macro is cache-first:

1. A compatible cache entry is used when `WORMX_REDO_CACHE` is not set.
2. `WORMX_OFFLINE=1` fails without connecting when no entry exists.
3. Otherwise, a configured database URL is used to generate and write the
   cache.

| Variable | Meaning |
|---|---|
| `DATABASE_URL` | Database URL, unless renamed with `wormx.database.url-var` |
| `WORMX_OFFLINE` | `true`/`1`: use the cache and fail if an entry is missing |
| `WORMX_OFFLINE_DIR` | Read/write cache in this directory first |
| `WORMX_REDO_CACHE` | `true`/`1`: skip the cache and read the database again |

Lookup checks `WORMX_OFFLINE_DIR`, the crate's cache directory, then the
workspace root's `.wormx/`. Commit `.wormx/` so CI builds do not need a
database. Apply SQLx migrations before refreshing the cache.

A `.env` file is read from the manifest directory and its ancestors; real
environment variables win. Cargo does not track `.env` reads made by proc
macros, so touch a source file or clean the crate after changing only `.env`.

## Generated code

`table!` emits:

- a row struct with SQL types mapped to Rust types;
- a `sqlx::FromRow` implementation that does not require the SQLx derive;
- a `wormx::Table` implementation with column and primary-key metadata;
- relation accessors and include types for foreign keys;
- `all()`, `fields()`, `new(...)`, and `get(...)` where applicable.

Per-table `derives(...)`, `attrs(...)`, `relation_attrs(...)`, and `columns`
customizations are documented in
[`wormx-macros`](https://github.com/strawberyy-coconut/wormx/blob/master/wormx-macros/README.md).

## Constraints and decisions

- Compile-time access is intentional: generated types come from the migrated
  schema.
- The cache is schema metadata, not a query cache.
- Cache format v1 is an internal format; invalid or mismatched entries are an
  error, not a silent reconnect.
- The code generator is shared between all drivers. Driver differences belong in
  the `TableDriver` functions, not in generated code.
- `wormx-macros-core` does not depend on a specific SQLx database driver.
