# Compile-time introspection

`wormx::table!` runs at compile time. It reads the migrated schema, asks a driver
to describe the table and read its constraints, then generates Rust code. The
macro is cache-first so builds do not require a live database once the schema
cache is present.

## Pipeline

```text
table!("schema.table")
        │
        ▼
parse table reference + per-table options
        │
        ▼
load wormx.toml / sqlx.toml and environment
        │
        ▼
cache lookup ── hit ──► deserialize Describe + Constraints
        │ miss
        ▼
select driver from URL scheme (or wormx.dialect)
        │
        ▼
driver.describe("SELECT * FROM ...")
driver.constraints(primary key, defaults, identity, generated, foreign keys)
        │
        ▼
write cache (schema metadata only)
        │
        ▼
shared code generator
```

The cache key covers the database name, schema, and table name. Cache entries
live in `table-<hash>.json` and contain the serialized `Describe` metadata plus
constraints. They never contain credentials or row data.

## Cache-first behavior

1. If a compatible cache entry exists and `WORMX_REDO_CACHE` is not set, use it
   and do not connect to the database.
2. If `WORMX_OFFLINE=1` is set, fail without connecting when no cache entry was
   found.
3. Otherwise, if the database URL is set, connect, generate, and write the
   cache.

Lookup checks `WORMX_OFFLINE_DIR`, the crate's configured cache directory, and
then the workspace root's `.wormx/`. The workspace cache is skipped when it is
the same directory as the configured cache.

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

[wormx.tables."public.users"]
struct = "User"
rust-path = "crate::models::User"

[macros.preferred-crates]
date-time = "chrono"
numeric = "rust_decimal"

[macros.type-overrides]
"UUID" = "uuid::Uuid"
"public.mood" = "crate::Mood"

[macros.table-overrides."public.users"]
"name" = "crate::UserName"
```

Type overrides match the SQL type name exactly or uppercased, then with the
table schema prefix. Table overrides match the qualified table name first, then
the bare table name. `rust-path` is used for relation target types that live in
another module.

### Environment variables

| Variable | Meaning |
|---|---|
| `DATABASE_URL` | Database URL, unless renamed with `wormx.database.url-var` |
| `WORMX_OFFLINE` | `true`/`1`: use the cache and fail if an entry is missing |
| `WORMX_OFFLINE_DIR` | Read/write cache in this directory first |
| `WORMX_REDO_CACHE` | `true`/`1`: skip the cache and read the database again |

The configured URL variable, `WORMX_OFFLINE`, `WORMX_OFFLINE_DIR`, and
`WORMX_REDO_CACHE` are emitted as `option_env!` dependencies in the generated
code, so changing them causes Cargo to rebuild. `wormx.toml` and `sqlx.toml` are
emitted as `include_str!` dependencies when they exist.

A `.env` file is read from the manifest directory and its ancestors. Real
environment variables take precedence. Cargo does not track `.env` reads made by
proc macros, so after changing only `.env`, touch a source file or clean the
crate.

## Per-table options

`table!` accepts an optional struct name and a block for table- and column-level
customizations:

```rust
wormx::table! {
    "dbo.source_file" as SourceFile {
        derives(Debug, Clone, serde::Serialize, utoipa::ToSchema),
        attrs(schema(as = SourceFileSchema)),
        relation_attrs(serde(skip), schema(ignore)),
        columns {
            publication_date {
                attrs(
                    serde(serialize_with = "crate::helpers::time::serde_dt_opt::serialize"),
                    schema(value_type = Option<String>, format = DateTime),
                ),
            },
            "type" as kind {
                attrs(serde(rename = "type"), schema(rename = "type")),
            },
        },
    }
}
```

- `as StructName` overrides `[wormx.tables."<table>".struct]` and the derived
  name.
- `derives(...)` replaces `[wormx.codegen] derives` for this table.
- `attrs(...)` adds attributes to the generated struct or field. The body is
  written without the surrounding `#[...]`.
- `relation_attrs(...)` adds attributes to every generated relation field.
- `columns { "db_column" as field_name { ... } }` renames a field and applies
  attributes. The column name may be written unquoted, and `as` is optional when
  only attributes are needed.

## The `TableDriver` seam

`wormx-macros-core` accepts a `TableDriver` — a describe function plus a
constraints function — so another backend can reuse the cache and code
generator:

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

fn describe_mssql(
    table: &wormx_core::TableRef,
    url: &str,
) -> Result<serde_json::Value, String> {
    // quote the table name for MSSQL, call describe_blocking, serialize Describe
}

fn constraints_mssql(
    url: &str,
    table: &wormx_core::TableRef,
    config: &wormx_core::Config,
) -> Result<wormx_macros_core::Constraints, Box<dyn std::error::Error + Send + Sync>> {
    // query sys.columns / sys.indexes
}
```

The built-in `wormx::table!` only wires up the bundled drivers. A third-party
backend can depend on `wormx-macros-core` directly and expose its own macro.

## Constraints and decisions

- **Compile-time access is intentional.** The schema is read when the crate
  builds. That gives generated types and column metadata without runtime
  reflection, at the cost of needing a database or cache during the build.
- **The cache is schema metadata, not a query cache.** It does not contain rows,
  credentials, or rendered SQL.
- **Caches are committed.** CI builds should not need a database. Refresh the
  cache explicitly with `WORMX_REDO_CACHE=1` after migrations.
- **`.env` changes need a rebuild trigger.** Cargo does not track proc-macro file
  reads.
- **Introspection is driver-owned.** Core does not know how to query
  `information_schema`, `sys.columns`, or `PRAGMA table_xinfo`.

## Future

- Add a machine-readable `WORMX_OFFLINE_*` or source-lock format if the JSON
  cache needs a stable public schema.
- Revisit whether cache lookup should honor `wormx.dialect` before checking
  caches from other enabled drivers.
- Consider exposing the introspection pipeline as a library command for use
  outside proc macros.
