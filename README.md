# wormx

**Typed SQL for Rust, generated from the schema you already have.**

`wormx` reads your migrated SQLx database and generates row types, table
metadata, and a typed query DSL. Keep writing SQL against real tables; let the
compiler catch column names, value types, and relation mistakes.

```rust
wormx::table!("public.users");

let users = db
    .query(Users::all())
    .filter(Users::fields().active().eq(true))
    .order_by(Users::fields().name().asc())
    .limit(20)
    .exec()
    .await?;
```

No schema DSL, no migration engine, and no runtime reflection. `wormx` sits on
top of SQLx and works with the schema you already migrated.

## Why wormx

- **SQL-first.** Your database remains the source of truth. `table!` reads the
  real schema and generated types match it.
- **Typed by default.** Columns, filters, projections, joins, inserts, updates,
  upserts, and returned rows are checked at compile time.
- **Relations that feel native.** Foreign keys generate `belongs_to` and
  `has_many` accessors, lazy queries, and nested eager loading.
- **Batched eager loading.** `.include(...)` loads related rows with chunked
  `IN` queries instead of one query per parent.
- **Escape hatches included.** Raw SQL, custom functions, and custom operators
  are normal parts of the API.
- **Four SQL backends.** PostgreSQL, SQLite, MySQL, and SQL Server share one
  typed API without pretending their SQL is identical.
- **Cache-friendly builds.** Commit the generated schema cache and build in CI
  without a database.

## Quick start

```toml
[dependencies]
wormx = { version = "0.2", features = ["postgres"] }
sqlx = { version = "0.9", default-features = false, features = ["runtime-tokio", "postgres"] }
tokio = { version = "1", features = ["full"] }
```

Apply your migrations first. wormx does not create or migrate schemas.

```bash
sqlx migrate run
cargo build
```

Define a table and use it:

```rust
wormx::table!("public.users");

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = sqlx::PgPool::connect(&std::env::var("DATABASE_URL")?).await?;
    let db = wormx::Db::new(pool, wormx::PostgresDialect);

    let users = db
        .query(Users::all())
        .filter(Users::fields().active().eq(true))
        .order_by(Users::fields().name().asc())
        .limit(20)
        .exec()
        .await?;

    for user in users {
        println!("{}", user.name);
    }

    Ok(())
}
```

Use `sqlite`, `mysql`, or `mssql` instead of `postgres` as needed. MSSQL also
needs `sqlx-mssql-rs`; see the
[MSSQL driver README](https://github.com/strawberyy-coconut/wormx/blob/master/wormx-drivers/wormx-drivers-mssql/README.md).

## Queries

Primary-key lookup, filtering, ordering, paging, and projections are all typed:

```rust
let user = db.query(Users::get(1)).first().await?;

let active_names: Vec<String> = db
    .query(Users::all())
    .filter(Users::fields().active().eq(true))
    .distinct()
    .select(Users::fields().name())
    .exec()
    .await?;
```

Subqueries and aggregates compose:

```rust
let active_ids = db
    .query(Users::all())
    .filter(Users::fields().active().eq(true))
    .select(Users::fields().id());

let count = db
    .query(Users::all())
    .filter(Users::fields().id().in_query(active_ids))
    .count()
    .await?;
```

## Writes

```rust
db.insert::<Users>()
    .value(Users::fields().name(), "Alice".to_owned())
    .value(Users::fields().active(), true)
    .exec()
    .await?;

let updated = db
    .update::<Users>()
    .set(Users::fields().active(), false)
    .filter(Users::fields().name().eq("Alice".to_owned()))
    .returning()
    .exec()
    .await?;
```

Upserts use the backend's native form — `ON CONFLICT`, `ON DUPLICATE KEY`, or
`MERGE`:

```rust
db.upsert::<Users>()
    .value(Users::fields().id(), user.id)
    .value(Users::fields().name(), "Alice".to_owned())
    .conflict_on(Users::fields().id())
    .update_from_input(Users::fields().name())
    .exec()
    .await?;
```

Batch writes are one statement each:

```rust
let inserted = db
    .insert_many::<Users>()
    .row(|row| row
        .value(Users::fields().name(), "Alice".to_owned())
        .value(Users::fields().active(), true))
    .row(|row| row
        .value(Users::fields().name(), "Bob".to_owned())
        .value(Users::fields().active(), false))
    .returning()
    .exec()
    .await?;
```

Transactions use the usual explicit shape:

```rust
let mut tx = db.begin().await?;

db.insert::<Todos>()
    .value(Todos::fields().user_id(), user.id)
    .value(Todos::fields().title(), "Inside a transaction".to_owned())
    .exec_in(&mut tx)
    .await?;

tx.commit().await?;
```

## Relations

Foreign keys generate accessors that are both a query source and a loaded-value
accessor:

```rust
let todos = db.query(user.todos()).exec().await?;
let owner = db.query(todos[0].user()).first().await?;

if let Some(loaded) = user.todos().loaded() {
    println!("{} loaded todos", loaded.len());
}
```

Nested eager loading is a `.include(...)` chain:

```rust
let users = db
    .query(Users::all())
    .include(
        Users::fields()
            .todos()
            .include(Todos::fields().user()),
    )
    .exec()
    .await?;

let todos = users[0].todos().loaded().expect("todos was included");
let owner = todos[0].user().loaded().expect("user was included");
```

Composite foreign keys, nullable keys, and rename-safe relation loading are
covered in [relations](https://github.com/strawberyy-coconut/wormx/blob/master/docs/design/relations.md).
Relation generation can also be restricted per table with `relations`,
`include-relations`, and `skip-relations`, so tables without a generated target
type do not force the whole FK graph into the build. Many-to-many is
deliberately not a generated relation type; a junction table is just another
typed table and composes with the query builder.

## Joins, CTEs, and windows

```rust
let active_ids = db
    .query(Users::all())
    .filter(Users::fields().active().eq(true))
    .select(Users::fields().id());

let users = db
    .query(Users::all())
    .with(wormx::Cte::new("active_ids", active_ids))
    .join(wormx::Join::inner_named(
        "active_ids",
        Users::fields()
            .id()
            .eq(wormx::Column::<i64>::new(None, "active_ids", "id")),
    ))
    .exec()
    .await?;
```

Joins accept tables, CTEs, and derived tables. `CROSS JOIN`, `USING`, and
`NATURAL JOIN` are available where the backend supports them:

```rust
let crossed = db
    .query(Users::all())
    .join(wormx::Join::cross(Todos::fields().table()))
    .select((Users::fields().name(), Todos::fields().title()))
    .exec()
    .await?;
```

Window functions use `.over(...)`:

```rust
let ranked: Vec<(String, i64)> = db
    .query(Users::all())
    .select((
        Users::fields().name(),
        wormx::row_number::<wormx::PostgresDialect>()
            .over(wormx::Window::new().order_by(Users::fields().id().asc())),
    ))
    .exec()
    .await?;
```

## Expressions and escape hatches

Filters compose with `.and(...)`, `.or(...)`, and `.not()`:

```rust
let filter = Users::fields()
    .active()
    .eq(true)
    .and(Users::fields().name().like("%a%"));

let users = db
    .query(Users::all())
    .filter(filter.or(Users::fields().email().is_null()))
    .exec()
    .await?;
```

Declare database functions and operators without changing wormx:

```rust
wormx::sql_function! {
    pub fn lower(value: String) -> String;
}

wormx::operator! {
    pub fn concat(left: String, right: String) -> String = "||";
    fixity = Infix;
    grouping = Operands;
}
```

`CASE`, `CAST`, and `COALESCE` are core expressions. Raw SQL remains available
when the DSL does not cover a query. Placeholder syntax follows the backend; the
example below uses the `?` style:

```rust
let users: Vec<Users> = db
    .sql("SELECT * FROM users WHERE active = ?")
    .bind(true)
    .fetch_all()
    .await?;
```

## Supported databases

| Database | Feature | Dialect |
|---|---|---|
| PostgreSQL | `postgres` | `wormx::PostgresDialect` |
| SQLite | `sqlite` | `wormx::SqliteDialect` |
| MySQL | `mysql` | `wormx::MySqlDialect` |
| SQL Server | `mssql` | `wormx::MssqlDialect` |

Backend-specific behavior and capability differences live with each driver:

- [PostgreSQL driver](https://github.com/strawberyy-coconut/wormx/blob/master/wormx-drivers/wormx-drivers-postgres/README.md)
- [SQLite driver](https://github.com/strawberyy-coconut/wormx/blob/master/wormx-drivers/wormx-drivers-sqlite/README.md)
- [MySQL driver](https://github.com/strawberyy-coconut/wormx/blob/master/wormx-drivers/wormx-drivers-mysql/README.md)
- [MSSQL driver](https://github.com/strawberyy-coconut/wormx/blob/master/wormx-drivers/wormx-drivers-mssql/README.md)

## How it works

wormx is built in layers:

- `wormx-core` holds the backend-neutral schema and plan layers, the expression
  AST, SQL renderers, and SQLx-backed execution.
- `wormx-macros-core` reads the schema, manages the compile-time cache, and
  generates types.
- `wormx-macros` is the `table!` entry point.
- `wormx-drivers` contains the built-in backends.

The root README stays focused on using the library. The design constraints and
future decisions live in [`docs/design/`](https://github.com/strawberyy-coconut/wormx/blob/master/docs/design/README.md):

- [Layering and capability policy](https://github.com/strawberyy-coconut/wormx/blob/master/docs/design/layering.md)
- [Compile-time introspection](https://github.com/strawberyy-coconut/wormx/blob/master/docs/design/compile-time-introspection.md)
- [Backends and renderers](https://github.com/strawberyy-coconut/wormx/blob/master/docs/design/backends.md)
- [Relations](https://github.com/strawberyy-coconut/wormx/blob/master/docs/design/relations.md)
- [SQL fidelity](https://github.com/strawberyy-coconut/wormx/blob/master/docs/design/sql-fidelity.md)

Crate-level details live in the crate READMEs:

- [`wormx-core`](https://github.com/strawberyy-coconut/wormx/blob/master/wormx-core/README.md)
- [`wormx-macros-core`](https://github.com/strawberyy-coconut/wormx/blob/master/wormx-macros-core/README.md)
- [`wormx-macros`](https://github.com/strawberyy-coconut/wormx/blob/master/wormx-macros/README.md)
- [`wormx-drivers`](https://github.com/strawberyy-coconut/wormx/blob/master/wormx-drivers/README.md)

## Examples and tests

Runnable examples live under `examples/`:

```bash
cargo run -p wormx-example-sqlite
cargo run -p wormx-example-postgres
cargo run -p wormx-example-mssql

# Standalone; start the compose MySQL service first.
cd examples/mysql && cargo run
```

The Postgres and MSSQL examples use the development servers from
`compose.dev.yaml`. Each example keeps its application in `src/app.rs` so the
SQL extensions can target that database's syntax directly.

Run the test suite with:

```bash
cargo test --workspace
```

The SQLite fixture creates its database and cache under Cargo's `OUT_DIR`. The
Postgres and MSSQL fixtures expect their development servers; override them with
`POSTGRES_URL` / `DATABASE_URL` or `MSSQL_URL`.

## License

MIT OR Apache-2.0.
