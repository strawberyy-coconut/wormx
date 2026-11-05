# wormx

`wormx` generates Rust row types and table metadata for existing SQLx databases.

The core produces typed plans. The dialect lowers them to backend requests and
owns the SQL rendering. The trait provides a generic SQL renderer as
overridable defaults, so SQL drivers only override what differs.

It targets SQL databases only. There is no NoSQL abstraction planned; the
dialect layer exists to handle SQL spelling differences, not to abstract away
non-SQL data models.

wormx is a query and schema-introspection layer **on top of SQLx**. It does not
manage schemas, migrations, or database lifecycle. Use SQLx's tooling for that
(for example `sqlx migrate`, `sqlx-cli`, or embedded migrations), then run
wormx against the already-migrated database. A typical workflow is:

```bash
sqlx database create
sqlx migrate run

# schema is current; wormx can describe it (or read its cache)
cargo build
```

```rust
wormx::table!("public.users");

fn print_table() {
    println!("{}", <Users as wormx::Table>::DEF.name);
}
```

`table!` connects to the database at compile time, asks the driver to describe
`SELECT * FROM the_table`, reads constraints (primary key, defaults, identity,
generated columns), and emits:

- a row struct with SQL types mapped to Rust types,
- a `sqlx::FromRow` implementation that does not require the `sqlx` derive,
- a `wormx::Table` implementation containing column and primary-key metadata.

## Status

Implemented:

- Postgres, SQLite, MySQL, and MSSQL introspection.
- `table!("table")` and `table!("schema.table")`.
- Compile-time schema cache in `.wormx/`.
- `wormx.toml` configuration, with `sqlx.toml` fallback for type overrides and
  preferred date/time and numeric crates.
- Type overrides for enums and custom Rust types.
- Typed query builder: filters, ordering, limit/offset, primary-key lookup.
- Generated `fields()`, `all()`, `get(...)`, and constructors.
- Generated `belongs_to` / `has_many` relations from foreign keys, including
  composite foreign keys.
- Lazy relation queries and eager loading with `.include(...)`.
- Nested eager loading: `.include(Users::fields().todos().include(Todos::fields().user()))`.
- Relation access through `user.todos().exec(&db)` and `user.todos().loaded()`.
- Expression composition: `.and(...)`, `.or(...)`, `.not()`, `COALESCE`.
- Custom SQL functions with `sql_function!`.
- Custom infix/prefix/postfix operators.
- Generated `Users::filter(...)` for dialect-neutral filters.
- Single-column projections with `.select(...)` and `IN` subqueries with `.in_query(...)`.
- `COUNT` and raw SQL escape hatches with `db.sql(...).bind(...)`.
- Insert, update, delete, and upsert builders, plus transactions and
  `RETURNING` where the dialect supports it.
- Grouping and `HAVING`.
- CTEs, window functions, and arbitrary joins over tables, CTEs, or derived tables.
- Runtime traits for dialect capabilities and typed expressions, including the
  extension points used for driver-specific SQL functions.

Many-to-many is deliberately not a generated relation type. In SQL it is two
one-to-many relationships through a junction table, and the existing DSL
expresses it directly:

```rust
let team_ids = db
    .query(UserTeams::all())
    .filter(UserTeams::fields().user_id().eq(user.id))
    .select(UserTeams::fields().team_id());

let teams = db
    .query(Teams::all())
    .filter(Teams::fields().id().in_query(team_ids))
    .exec()
    .await?;
```

## Usage

```toml
[dependencies]
wormx = { version = "0.1", features = ["postgres"] }
sqlx = { version = "0.9", default-features = false, features = ["runtime-tokio", "postgres"] }
```

Use `sqlite`, `mysql`, or `mssql` instead of `postgres` as needed.

wormx does not run or generate migrations. Keep using SQLx's migration support
(`sqlx migrate`, `sqlx-cli`, or `sqlx::migrate!`) to create and update the
schema; `table!` only reads the resulting schema.

For MSSQL, also depend on `sqlx-mssql-rs`:

```toml
[dependencies]
wormx = { version = "0.1", features = ["mssql"] }
sqlx-mssql-rs = "0.1"
tokio = { version = "1", features = ["full"] }
```

MSSQL connection URLs use `mssql://`. The development default is:

```text
mssql://sa:Password1!@mssql:1433/master?encrypt=on&trust_certificate=true
```

The MSSQL `json` feature is currently a no-op because `sqlx-mssql-rs` 0.1.0
enables its JSON module without the matching `serde` dependency. Other MSSQL
type features (`chrono`, `uuid`, `bigdecimal`, `rust_decimal`) are forwarded.

The generated code refers to `::wormx`, so `wormx` must be a direct dependency.

## Querying

```rust
let db = wormx::Db::new(pool, wormx::PostgresDialect);

let users = db
    .query(Users::all())
    .filter(Users::fields().active().eq(true))
    .order_by(Users::fields().name().asc())
    .limit(20)
    .exec()
    .await?;

let user = db
    .query(Users::get(1))
    .first()
    .await?;
```

`Users::new(...)` constructs a row from column values and marks all relations
as unloaded. `Users::get(...)` is generated when the table has a single-column
primary key.

## Writing

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
    .exec_returning()   // Postgres and SQLite
    .await?;

db.delete::<Users>()
    .filter(Users::fields().id().eq(1_i64))
    .exec()
    .await?;
```

Insert and update values accept either Rust values or expressions; use
`.value_raw(...)` / `.set_raw(...)` for SQL expressions such as `NOW()`.
`exec_returning()` decodes returned rows and is only used on dialects with
`RETURNING` support. Plain `exec()` returns the driver's `QueryResult`.

Upserts use a dialect renderer. PostgreSQL and SQLite use `ON CONFLICT`, MySQL
uses `ON DUPLICATE KEY UPDATE`, and SQL Server uses `MERGE ... WITH (HOLDLOCK)`:

```rust
db.upsert::<Users>()
    .value(Users::fields().id(), user.id)
    .value(Users::fields().name(), "Alice".to_owned())
    .conflict_on(Users::fields().id())
    .update(Users::fields().name(), "Alice".to_owned())
    .exec()
    .await?;
```

`conflict_on` is the conflict target for PostgreSQL/SQLite and the join key for
SQL Server's `MERGE`; MySQL ignores it because `ON DUPLICATE KEY` chooses the
unique index itself. `exec_returning` remains dialect-gated and is not emitted
for dialects that report `returning: false`.

Transactions use `begin`, `exec_in`, and `commit`/`rollback`:

```rust
let mut tx = db.begin().await?;

db.insert::<Todos>()
    .value(Todos::fields().user_id(), user.id)
    .value(Todos::fields().title(), "Inside a transaction".to_owned())
    .exec_in(&mut tx)
    .await?;

db.query(Users::all())
    .filter(Users::fields().id().eq(user.id))
    .exec_in(&mut tx)
    .await?;

tx.commit().await?;
```

The transaction rolls back on drop unless committed.

## CTEs, windows, and joins

CTEs are added with `.with(...)`:

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

Joins accept a generated table, a named CTE, or a derived table:

```rust
let todo_users = db
    .query(Todos::all())
    .select(Todos::fields().user_id());

let users = db
    .query(Users::all())
    .join(wormx::Join::inner_derived(
        todo_users,
        "todo_users",
        Users::fields()
            .id()
            .eq(wormx::Column::<i64>::new(None, "todo_users", "user_id")),
    ))
    .distinct()
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

`GROUP BY` and `HAVING` are available on projected selects.

### Table hints and statement options

Core has only an opaque source suffix and statement tail. It does not know what
`ROWLOCK`, `READPAST`, or `OPTION (RECOMPILE)` mean. SQL Server supplies the
typed builders from its driver crate:

```rust
use wormx::{MssqlStatementExt, MssqlTableExt, TableHint};

let rows = db
    .query(Users::all())
    .hint("ROWLOCK, READPAST")
    .exec()
    .await?;

db.update::<Users>()
    .hint(TableHint::RowLock)
    .set(Users::fields().active(), false)
    .filter(Users::fields().id().eq(1_i64))
    .exec()
    .await?;
```

Hints also work on `TableSource` for joins:

```rust
let source = Todos::alias("t").table().hint(TableHint::ReadPast);
let joined = db
    .query(Users::all())
    .join(wormx::Join::inner(source, Users::fields().id().eq(Todos::alias("t").user_id())))
    .exec()
    .await?;
```

Statement-level options are available on queries and counts through
`MssqlStatementExt`:

```rust
let claim = db
    .query(Jobs::all())
    .filter(Jobs::fields().status().eq("READY"))
    .option("RECOMPILE")
    .first()
    .await?;
```

The string is the content of `OPTION (...)`, without the wrapper. Dialects that
do not override the core hooks ignore opaque suffixes and tails. A driver can
add its own extension trait for its own syntax without changing core.

## Relations

Relations are generated from foreign keys. For:

```sql
CREATE TABLE todos (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users(id),
    title TEXT NOT NULL
);
```

the generated API includes:

```rust
let todos = db.query(user.todos()).exec().await?;
let owner = db.query(todos[0].user()).first().await?;
```

Relation fields are private and stored as `Deferred` values. The relation
accessor is both the query accessor and the loaded-value accessor:

```rust
let todos = user.todos().exec(&db).await?;   // lazy query
let loaded = user.todos().loaded();          // Option<&Vec<Todos>>
```

For `belongs_to`:

```rust
let owner = todo.user().first(&db).await?;   // Option<Users>
let loaded = todo.user().loaded();           // Option<&Users>
```

Eager loading supports nested includes:

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

let todos = users[0]
    .todos()
    .loaded()
    .expect("todos was included");
let owner = todos[0]
    .user()
    .loaded()
    .expect("user was included by the nested include");
```

Nullable foreign keys generate `Deferred<Option<Target>>` and
`BelongsTo::optional(...)`. Multiple foreign keys from the same source table
get qualified names such as `user_todos` and `reviewer_todos`.

Composite foreign keys generate one relation whose key is a tuple. The
accessor returns a generated handle with the same `loaded()`/`expect_loaded()`
API and implements `IntoSelect`, so it works with `db.query(...)` and
`.include(...)`:

```rust
let orders = db
    .query(Accounts::all())
    .include(Accounts::fields().orders())
    .exec()
    .await?;

let account = db
    .query(order.account())
    .first()
    .await?;
```

Relation target types are derived from the target table name. Use
`rust-path` when the target struct lives in another module:

```toml
[wormx.tables."public.users"]
struct = "User"
rust-path = "crate::models::User"
```

## Expressions and extension

Filters compose:

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

Declare database functions without touching wormx core:

```rust
wormx::sql_function! {
    pub fn lower(value: String) -> String;
}

let users = db
    .query(Users::all())
    .filter(lower(Users::fields().name()).eq("alice"))
    .exec()
    .await?;
```

`sql_function!` accepts columns, typed expressions, and Rust values for its
arguments. Use `sql = "..."` when the SQL name differs from the Rust name.

Custom operators follow the same pattern:

```rust
wormx::operator! {
    pub fn concat(left: String, right: String) -> String = "||";
    fixity = Infix;
    grouping = Operands; // Operands | None | Precedence(n)
}

wormx::operator! {
    pub fn is_unknown(value: String) -> bool = "IS UNKNOWN";
    fixity = Postfix;
    grouping = Operands;
}
```

`Operands` is the safe default. `None` opts out of parentheses and is only for
operators whose grammar forbids them. `Precedence(n)` is accepted for forward
compatibility and currently renders like `Operands`.

This is the same extension model Diesel documents: operators and functions are
normal types/functions supplied by the application or driver crate, not enum
variants that require a core change. A driver can expose its own operators and
functions from its crate.

Custom operators parenthesize their operands by default, so a declared prefix
operator is safe inside a larger predicate:

```rust
wormx::operator! {
    pub fn not_(value: bool) -> bool = "NOT";
    fixity = Prefix;
    grouping = Operands;
}

// not_(active.eq(true).and(name.eq("Alice"))) renders as
// NOT ((... = ...) AND (... = ...))
```

Use `Expr::group()` when you want an explicit pair of parentheses around an
expression you built by hand.

`Expr::raw` emits text verbatim and does not bind it. Never interpolate user
input into `raw`; use `.eq(value)` or `Expr::bind(value)` instead. Reserve `raw`
for literals and SQL constructs that must stay literal, such as a
filtered-index predicate (`status <> N'FINISHED'`).

`COALESCE` is SQL standard, so it lives in the core expression layer rather
than a driver crate:

```rust
db.query(Users::all())
    .filter(wormx::coalesce(Users::fields().email(), "none").eq("none"))
    .exec()
    .await?;
```

Filters can also be built without a database handle:

```rust
let query = Users::filter(Users::fields().active().eq(true));
let users = db.query(query).exec().await?;
```

`DISTINCT`, joins, grouping, and aliases:

```rust
let t = Todos::alias("t");

let rows: Vec<(String, String)> = db
    .query(Users::all())
    .join(wormx::Join::inner(
        t.table(),
        Users::fields().id().eq(t.user_id()),
    ))
    .select((Users::fields().name(), t.title()))
    .order_by(t.id().asc())
    .exec()
    .await?;

let names: Vec<String> = db
    .query(Users::all())
    .distinct()
    .select(Users::fields().name())
    .exec()
    .await?;

let grouped: Vec<(i64, i64)> = db
    .query(Todos::all())
    .select((Todos::fields().user_id(), Todos::fields().id().count()))
    .group_by(Todos::fields().user_id())
    .having(Todos::fields().id().count().gt(1_i64))
    .exec()
    .await?;
```

Projections and subqueries. `select` returns exactly the projected shape:

```rust
// One column -> Vec<i64>
let ids: Vec<i64> = db
    .query(Todos::all())
    .select(Todos::fields().id())
    .exec()
    .await?;

// Multiple columns -> Vec<(i64, String)>
let rows: Vec<(i64, String)> = db
    .query(Todos::all())
    .select((Todos::fields().id(), Todos::fields().title()))
    .exec()
    .await?;

// Computed columns work too.
let rows: Vec<(String, i64)> = db
    .query(Users::all())
    .select((lower(Users::fields().name()), Users::fields().id()))
    .exec()
    .await?;

let subquery = db
    .query(Todos::all())
    .filter(Todos::fields().user_id().eq(1_i64))
    .select(Todos::fields().user_id());

let users = db
    .query(Users::all())
    .filter(Users::fields().id().in_query(subquery))
    .exec()
    .await?;
```

Custom SQL remains available when the DSL does not cover a query:

```rust
let rows: Vec<Users> = db
    .sql("SELECT * FROM users WHERE active = ?")
    .bind(true)
    .fetch_all()
    .await?;

let count = db
    .query(Users::all())
    .filter(Users::fields().active().eq(true))
    .count()
    .await?;
```

This is deliberate: the ORM should make the common path typed, while still
letting an application represent a query the DSL has not learned yet.

### SQL fidelity today

Supported directly:

- `SELECT` projections, including tuples, computed expressions, and aggregates
- `DISTINCT`
- `WHERE` with SQL precedence preserved by explicit grouping
- `IN`, `NOT IN`, `IN (subquery)`
- `JOIN` / `LEFT JOIN` / `RIGHT JOIN` / `FULL JOIN` (`FULL JOIN` only for
  dialects implementing `SupportsFullJoin`; MySQL and MariaDB do not)
- `GROUP BY` and `HAVING`
- `ORDER BY`, `LIMIT`, `OFFSET`
- `COUNT(*)`, `COUNT(expr)`, `COUNT(DISTINCT expr)`
- CTEs (`.with(...)`) and window functions (`.over(...)`)
- insert, update, delete, and upsert builders
- `RETURNING` on dialects that support it (`exec_returning`)
- driver-specific `LIMIT/OFFSET` spelling, plus opaque source suffixes and tails
  for dialect extensions such as SQL Server hints and `OPTION (...)`
- explicit grouping for custom operators, plus `Expr::group()`

Not modeled yet:

- `UNION` / `INTERSECT` / `EXCEPT`
- column aliases and ordered null placement

Those are the next places where the builder stops simulating SQL and starts
hiding it. Raw SQL exists so applications are not blocked in the meantime.

## Configuration

`wormx.toml` is read from the crate root.

```toml
[wormx]
cache-dir = ".wormx"

[wormx.database]
url-var = "DATABASE_URL"

[wormx.codegen]
derives = ["Debug", "Clone"]
visibility = "pub"
from-row = true

# Rename a generated struct.
[wormx.tables."public.users"]
struct = "User"

# Map a SQL type to a Rust type.
[macros.type-overrides]
"uuid" = "uuid::Uuid"
"public.mood" = "crate::Mood"

# Map one column.
[macros.table-overrides."public.users"]
"name" = "crate::UserName"
```

`sqlx.toml` is also read. Its `[common]`, `[macros.preferred-crates]`,
`[macros.type-overrides]`, and `[macros.table-overrides]` settings are used as
defaults; `wormx.toml` wins on conflicts.

The Rust type must implement the SQLx traits for the selected database:

```rust
#[derive(Debug, Clone, sqlx::Type)]
#[sqlx(type_name = "mood", rename_all = "snake_case")]
pub enum Mood {
    Happy,
    Sad,
}
```

with:

```toml
[macros.type-overrides]
"public.mood" = "crate::Mood"
```

## Cache and offline builds

The macro is cache-first:

1. If a compatible cache entry exists, use it and do not connect to the database.
2. Otherwise, if the database URL is set, connect, generate, and write the cache.
3. If `WORMX_OFFLINE=1` is set, never connect.

Cache files live in `.wormx/table-<hash>.json` and contain schema metadata only,
never credentials or row data. The hash covers the driver name, schema, and
table name. A cache entry is written to the crate's configured cache directory
(`wormx.cache-dir`, default `.wormx/`), or to `WORMX_OFFLINE_DIR` when set.
Lookup checks `WORMX_OFFLINE_DIR`, the crate's cache directory, and then the
workspace root's `.wormx/`.

Commit `.wormx/` to version control so CI builds do not need a database. Apply
SQLx migrations before generating or refreshing the cache, so the cache matches
the schema the application expects. To refresh an existing entry, run the build
again with `WORMX_REDO_CACHE=1` and a valid database URL.

Environment variables:

| Variable | Meaning |
|---|---|
| `DATABASE_URL` | Database URL, unless renamed with `wormx.database.url-var` |
| `WORMX_OFFLINE` | `true`/`1`: use the cache and fail if an entry is missing |
| `WORMX_OFFLINE_DIR` | Read/write cache in this directory first |
| `WORMX_REDO_CACHE` | `true`/`1`: skip the cache and read the database again |

`DATABASE_URL`, `WORMX_OFFLINE`, `WORMX_OFFLINE_DIR`, and
`WORMX_REDO_CACHE` are also emitted as `option_env!` dependencies in the
generated code, so changing them causes Cargo to rebuild the crate.

A `.env` file is read from the manifest directory and its ancestors. Real
environment variables take precedence. Cargo does not track file reads made by
proc macros, so after changing only `.env`, touch a source file or clean the
crate to make the macro run again.

## Extending a dialect

Built-in dialect markers are `wormx::PostgresDialect`, `wormx::SqliteDialect`,
`wormx::MySqlDialect`, and `wormx::MssqlDialect`.

`Dialect` identifies the backend and selects one renderer type per logical
operation. `SqlSyntax` holds the generic spelling defaults, and small marker
traits describe optional capabilities. A SQL driver implements them directly and
points each operation at the generic SQL renderer:

```rust
pub struct MssqlDialect;

impl wormx::SqlSyntax for MssqlDialect {}

impl wormx::Dialect for MssqlDialect {
    type DB = sqlx_mssql_rs::Mssql;

    type Select = wormx::SqlSelect;
    type Insert = wormx::SqlInsert;
    type Update = wormx::SqlUpdate;
    type Delete = wormx::SqlDelete;
    type Upsert = wormx::SqlUpsert;

    fn quote_ident(&self, name: &str) -> String {
        format!("[{}]", name.replace(']', "]]"))
    }
}
```

The operation traits are:

- `SelectRenderer` — row queries, `COUNT(expr)`, and `COUNT(*)`.
- `InsertRenderer`, `UpdateRenderer`, `DeleteRenderer`, `UpsertRenderer` — DML
  rendering. Each receives its plan (`InsertPlan`, `UpdatePlan`, `DeletePlan`,
  `UpsertPlan`) and writes the command string. The generic `SqlInsert`,
  `SqlUpdate`, `SqlDelete`, and `SqlUpsert` cover standard SQL.

A driver can replace any operation by implementing its trait and assigning the
type:

```rust
#[derive(Default)]
pub struct MssqlSelect;

impl wormx::SelectRenderer<MssqlDialect> for MssqlSelect {
    fn write<P>(
        &self,
        plan: wormx::SelectPlan<'_, MssqlDialect, P>,
        writer: &mut wormx::SqlWriter<'_, MssqlDialect>,
    ) -> sqlx::Result<()>
    where
        P: wormx::ProjectionWriter<MssqlDialect>,
    {
        // Full control over clause order and syntax.
        todo!()
    }

    fn write_count<P>(
        &self,
        plan: wormx::SelectPlan<'_, MssqlDialect, P>,
        writer: &mut wormx::SqlWriter<'_, MssqlDialect>,
        distinct: bool,
    ) -> sqlx::Result<()>
    where
        P: wormx::ProjectionWriter<MssqlDialect> + wormx::CountProjection<MssqlDialect>,
    {
        todo!()
    }

    fn write_count_star(
        &self,
        plan: wormx::CountPlan<'_, MssqlDialect>,
        writer: &mut wormx::SqlWriter<'_, MssqlDialect>,
    ) -> sqlx::Result<()> {
        todo!()
    }
}

impl wormx::Dialect for MssqlDialect {
    type Select = MssqlSelect;
    // ...
}
```

The associated type avoids specialization: a dialect chooses a renderer type
instead of trying to override a blanket implementation. `SqlSelect` is itself a
generic SQL renderer, so most drivers use it unchanged.

An external renderer can reproduce the generic shape. `Join::render`,
`TableSource::render`, `write_from_filters`, `write_groups_having`,
`write_orders`, and `write_tail` are public, so a renderer can override one
clause without copying the whole body. `SelectPlan` and `CountPlan` expose the
base table as a `TableSource`, including aliases and opaque suffixes.

For smaller differences, the `SqlSyntax` extension trait exposes overridable
spelling hooks. `Dialect` itself stays small:

```rust
impl wormx::SqlSyntax for MssqlDialect {
    fn write_count_star(writer: &mut wormx::SqlWriter<Self>) {
        writer.push("COUNT_BIG(*)");
    }

    // Write the function name only. The renderer adds `(` and `DISTINCT`.
    fn write_count_start(_distinct: bool, writer: &mut wormx::SqlWriter<Self>) {
        writer.push("COUNT_BIG");
    }

    // Source suffixes and statement tails are opaque in core. SQL Server gives
    // them `WITH (...)` and `OPTION (...)` meaning; the defaults emit nothing.
    fn write_source_suffix(&self, suffix: &str, writer: &mut wormx::SqlWriter<Self>) {
        writer.push(" WITH (");
        writer.push(suffix);
        writer.push(")");
    }

    fn write_tail(&self, tail: &str, writer: &mut wormx::SqlWriter<Self>) {
        writer.push(" OPTION (");
        writer.push(tail);
        writer.push(")");
    }

    fn write_limit_offset(
        &self,
        has_order: bool,
        limit: Option<u64>,
        offset: Option<u64>,
        out: &mut String,
    ) {
        // MSSQL OFFSET/FETCH, inserting ORDER BY (SELECT NULL) when needed.
    }
}
```

Capabilities are marker traits, not a separate boolean table. For example,
`SupportsReturning` gates `exec_returning`, `SupportsFullJoin` gates
`Join::full`, and `SupportsUpsert` gates `Db::upsert`; an unsupported operation
is a compile-time error instead of a rendered statement that the database
rejects.

```rust
impl wormx::SupportsFullJoin for MssqlDialect {}
impl wormx::SupportsFullTextSearch for MssqlDialect {}
impl wormx::SupportsUpsert for MssqlDialect {}
```

Compile-time schema inspection remains in `TableDriver`, not `Dialect`.

The AST and typed query builder live in core. The literal SQL text is produced
by the dialect layer:

- `SELECT`, `FROM`, `WHERE`, `AND`, `OR`
- `JOIN`, `ON`, `AS`
- `GROUP BY`, `HAVING`, `ORDER BY`, `ASC`, `DESC`
- `COUNT`, `DISTINCT`, `IN`, `NOT IN`, `IS NULL`, `LIKE`
- boolean literals, operators, functions
- identifier quoting, placeholders, limit/offset, `RETURNING`, and upsert

That means a driver can change SQL spelling without patching the query builder.

wormx is a sqlx library. A driver implements sqlx's `Database`, `Connection`,
`Executor`, `Row`, `Arguments`, and related traits, plus wormx's `Dialect`,
`SqlSyntax`, and its operation renderers. sqlx owns execution; the dialect layer owns the command
string. That string is opaque to sqlx, so a driver can interpret it as SQL or as
its own command language. No separate executor layer is planned.

A dialect-specific function is defined like this:

```rust
use wormx::{Expr, Function, SqlWriter};

pub struct Contains;

impl Function<MssqlDialect> for Contains {
    type Args = (Expr<String, MssqlDialect>, Expr<String, MssqlDialect>);
    type Output = bool;
    const NAME: &'static str = "CONTAINS";

    fn render(args: &mut Self::Args, w: &mut SqlWriter<'_, MssqlDialect>) {
        w.push("CONTAINS(");
        args.0.render(w).unwrap();
        w.push(", ");
        args.1.render(w).unwrap();
        w.push(")");
    }
}

pub fn contains<C, Q>(column: C, query: Q) -> Expr<bool, MssqlDialect>
where
    C: Into<Expr<String, MssqlDialect>>,
    Q: Into<Expr<String, MssqlDialect>>,
{
    wormx::call::<MssqlDialect, Contains>((column.into(), query.into()))
}
```

`Function` is defined by wormx, but the marker type is yours, so the orphan rule
is satisfied without changing wormx. Add `impl wormx::Textual for MyTextType {}`
to let custom text types participate in text functions.

## Adding a macro adapter for another driver

`wormx-macros-core` accepts a driver through `TableDriver`. A driver adapter can
be as thin as:

```rust
const DRIVER: TableDriver = TableDriver::new_with_fns::<sqlx_mssql_rs::Mssql>(
    &["mssql"], // aliases, no core patch needed
    describe_mssql,
    constraints_mssql,
);

#[proc_macro]
pub fn table(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as TableMacroInput);
    match wormx_macros_core::expand_table(input, &[&DRIVER]) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

fn describe_mssql(
    table: &wormx_core::TableRef,
    url: &str,
) -> Result<serde_json::Value, String> {
    // quote the table name for MSSQL, call the driver's describe_blocking,
    // and serialize Describe<Mssql>
}

fn constraints_mssql(
    url: &str,
    table: &wormx_core::TableRef,
    config: &wormx_core::Config,
) -> Result<wormx_macros_core::Constraints, Box<dyn std::error::Error + Send + Sync>> {
    // query sys.columns / sys.indexes
}
```

This mirrors how `sqlx-mssql-rs-macros` plugs into `sqlx-macros-core`.

## Workspace

- `wormx-core` — runtime metadata, config, naming, dialect and expression traits.
- `wormx-macros-core` — generic schema fetching, cache, type mapping and code generation.
- `wormx-macros` — the `table!` proc macro.
- `wormx-drivers` — umbrella crate for built-in drivers.
  - `wormx-drivers/wormx-drivers-postgres` — Postgres dialect and macro introspection.
  - `wormx-drivers/wormx-drivers-sqlite` — SQLite dialect and macro introspection.
  - `wormx-drivers/wormx-drivers-mysql` — MySQL dialect and macro introspection.
  - `wormx-drivers/wormx-drivers-mssql` — MSSQL dialect and macro introspection on `sqlx-mssql-rs`.
- `wormx` — application facade.
- `tests/sqlite-fixture` — end-to-end compile and runtime test using SQLite.
- `tests/postgres-fixture` — end-to-end test against the development Postgres.
- `tests/mssql-fixture` — end-to-end test against the development SQL Server.
- `examples/sqlite`, `examples/postgres`, `examples/mssql` — workspace examples,
  each with its own `app.rs` and driver setup.
- `examples/mysql` — standalone MySQL example (it has its own workspace so the
  main workspace does not require a live MySQL during a build).

## Tests

```bash
cargo test --workspace
```

The SQLite fixture creates its database and cache under Cargo's `OUT_DIR`, so
the test does not leave files in the repository. The Postgres and MSSQL fixtures
expect their development servers to be available; override the server with
`POSTGRES_URL` or `MSSQL_URL`.

Run the examples with:

```bash
cargo run -p wormx-example-sqlite
cargo run -p wormx-example-postgres
cargo run -p wormx-example-mssql

# Standalone; start the compose MySQL service and set MYSQL_URL first.
cd examples/mysql && cargo run
```

Each example keeps the application in its own `src/app.rs`, so the SQL
extenders can target the database's syntax directly. `main.rs` only creates the
pool and dialect. The apps demonstrate nested eager loading,
insert/update/delete, transactions, and custom SQL functions/operators.

By default the examples keep their schema cache in `OUT_DIR`, like the fixtures.
To generate a cache in the repository instead later, run:

```bash
WORMX_REPO_CACHE=1 cargo run -p wormx-example-sqlite
```

That writes `examples/sqlite/.wormx/table-<hash>.json`. Use
`WORMX_REPO_CACHE=1 WORMX_REDO_CACHE=1` to force a refresh. The same switch works
for the Postgres, MSSQL, and MySQL examples.
