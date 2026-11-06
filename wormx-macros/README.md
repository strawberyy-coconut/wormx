# wormx-macros

The `table!` procedural macro for [`wormx`](https://crates.io/crates/wormx).

Most users get this macro through the `wormx` facade:

```toml
[dependencies]
wormx = { version = "0.2", features = ["postgres"] }
```

then:

```rust
wormx::table!("public.users");
```

The macro reads the migrated schema at compile time (or from the cache),
generates a row type, and wires up table metadata and foreign-key relations.

## Basic forms

```rust
wormx::table!("users");
wormx::table!("public.users");
wormx::table!("public.users" as User);
```

`table!("users")` derives the Rust struct name from the table name. `as User`
overrides it.

## Per-table customization

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
- `derives(...)` replaces `[wormx.codegen] derives` for this table. Use
  `derives()` to emit no derives.
- `attrs(...)` adds attributes to the generated struct or field. Write the body
  without the surrounding `#[...]`; attributes are forwarded as written.
- `relation_attrs(...)` adds attributes to every generated relation field, for
  example `relation_attrs(serde(skip), schema(ignore))`.
- `columns { "db_column" as field_name { ... } }` renames a generated field and
  applies attributes. The column name may be unquoted, and `as` is optional when
  only attributes are added.

## Generated API

For a table `users`, the macro emits a row struct plus:

- `Users::all()` — a dialect-neutral table query;
- `Users::fields()` — typed column handles and relation includes;
- `Users::alias("u")` — column handles qualified by an alias;
- `Users::new(...)` — constructs a row and marks relations unloaded;
- `Users::get(...)` — single-column primary-key lookup, when applicable;
- `belongs_to` / `has_many` accessors derived from foreign keys.

Relation details are documented in
[relations](https://github.com/strawberyy-coconut/wormx/blob/master/docs/design/relations.md).

## Cache and configuration

`table!` is cache-first and can build offline from a committed `.wormx/` cache.
Configuration lives in `wormx.toml` (with `sqlx.toml` fallbacks), and cache
behavior is controlled by `WORMX_OFFLINE`, `WORMX_OFFLINE_DIR`, and
`WORMX_REDO_CACHE`.

See
[`wormx-macros-core`](https://github.com/strawberyy-coconut/wormx/blob/master/wormx-macros-core/README.md)
for the full pipeline, configuration keys, and environment variables.

## Constraints

- Migrations must run before the build that creates or refreshes the cache.
- Generated code refers to `::wormx`, so `wormx` must be a direct dependency.
- The schema cache contains metadata only; commit it so CI does not need a
  database.
- The macro does not generate or run migrations.
