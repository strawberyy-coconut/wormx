# Backends and renderers

A backend is an implementation of SQLx's database traits plus a `Dialect` and
its renderer types. SQL backends can reuse the built-in SQL renderers; a
different query language supplies its own.

## Dialect surface

`Dialect` identifies the backend and selects one renderer type per logical
operation:

```rust
pub struct PostgresDialect;

impl wormx::SqlSyntax for PostgresDialect {}

impl wormx::Dialect for PostgresDialect {
    type DB = sqlx_postgres::Postgres;

    type Select = wormx::SqlSelect;
    type Insert = wormx::SqlInsert;
    type Update = wormx::SqlUpdate;
    type Delete = wormx::SqlDelete;
    type Upsert = wormx::SqlUpsert;

    fn quote_ident(&self, name: &str) -> String {
        format!("\"{}\"", name.replace('"', "\"\""))
    }
}
```

Not every operation can use a generic renderer. SQL Server assigns
`MssqlUpsert`, which emits `MERGE ... WITH (HOLDLOCK)` instead of
`SqlUpsert`'s `ON CONFLICT`.

The operation traits are:

- `SelectRenderer` — row queries, `COUNT(expr)`, and `COUNT(*)`.
- `InsertRenderer`, `UpdateRenderer`, `DeleteRenderer`, `UpsertRenderer` — DML
  rendering. Each receives its plan (`InsertPlan`, `UpdatePlan`, `DeletePlan`,
  `UpsertPlan`) and writes the command string.

## Replacing a renderer

A driver can replace any operation by implementing its trait and assigning the
type:

```rust
#[derive(Default)]
pub struct MssqlSelect;

impl wormx::SelectRenderer<MssqlDialect> for MssqlSelect {
    fn write<P>(
        &self,
        plan: wormx::SelectPlan<'_, MssqlDialect, P>,
        writer: &mut wormx::SqlWriter<MssqlDialect>,
    ) -> sqlx::Result<()>
    where
        P: wormx::Projection<MssqlDialect>,
    {
        // Full control over clause order and syntax.
        todo!()
    }

    fn write_count<P>(
        &self,
        plan: wormx::SelectPlan<'_, MssqlDialect, P>,
        writer: &mut wormx::SqlWriter<MssqlDialect>,
        distinct: bool,
    ) -> sqlx::Result<()>
    where
        P: wormx::Projection<MssqlDialect> + wormx::CountProjection<MssqlDialect>,
    {
        todo!()
    }

    fn write_count_star(
        &self,
        plan: wormx::CountPlan<'_, MssqlDialect>,
        writer: &mut wormx::SqlWriter<MssqlDialect>,
    ) -> sqlx::Result<()> {
        todo!()
    }
}
```

The associated type avoids specialization: a dialect chooses a renderer type
instead of trying to override a blanket implementation. `SqlSelect` is itself a
generic SQL renderer, so most drivers use it unchanged.

An external renderer can reproduce the generic shape. `Join::render`,
`TableSource::render`, `write_from_filters`, `write_groups_having`,
`write_orders`, and `write_tail` are public, so a renderer can override one
clause without copying the whole body.

## Smaller spelling hooks

`SqlSyntax` exposes overridable spelling hooks. `Dialect` stays small:

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

Opaque source suffixes and statement tails are the generic escape hatches. A
driver that understands a suffix can render it; the default ignores it.

## Extension functions and operators

Application code can declare database functions without touching core:

```rust
wormx::sql_function! {
    pub fn lower(value: String) -> String;
}

wormx::operator! {
    pub fn concat(left: String, right: String) -> String = "||";
    fixity = Infix;
    grouping = Operands; // Operands | None | Precedence(n)
}
```

For dialect-specific rendering, implement `Function` with a marker type. The
marker belongs to the driver crate, so the orphan rule is satisfied:

```rust
use wormx::{Expr, Function, IntoExpr, MssqlDialect, SqlWriter};

pub struct Contains;

impl Function<MssqlDialect> for Contains {
    type Args = (Expr<String, MssqlDialect>, Expr<String, MssqlDialect>);
    type Output = bool;
    const NAME: &'static str = "CONTAINS";

    fn render(args: &mut Self::Args, w: &mut SqlWriter<MssqlDialect>) -> sqlx::Result<()> {
        w.push("CONTAINS(");
        args.0.render(w)?;
        w.push(", ");
        args.1.render(w)?;
        w.push(")");
        Ok(())
    }
}

pub fn contains<C, Q>(column: C, query: Q) -> Expr<bool, MssqlDialect>
where
    C: IntoExpr<String, MssqlDialect>,
    Q: IntoExpr<String, MssqlDialect>,
{
    wormx::call::<MssqlDialect, Contains>((column.into_expr(), query.into_expr()))
}
```

`Function` is defined by wormx, but the marker type is yours. `Operator` follows
the same pattern. `wormx::Textual` is available to driver extensions for their
own text types.

## Adding a new driver

A driver implements SQLx's `Database`, `Connection`, `Executor`, `Row`,
`Arguments`, and related traits, plus wormx's `Dialect` and its operation
renderers. A SQL driver also implements `SqlSyntax` and can reuse the SQL
renderers.

For compile-time introspection, see
[compile-time introspection](compile-time-introspection.md#the-tabledriver-seam).
The short version is that a backend provides a `TableDriver` and either wires it
into its own `table!` macro or calls `expand_table` directly.

## Constraints and decisions

- **Core does not own vendor spelling.** Driver syntax belongs in a driver crate,
  a `SqlSyntax` hook, `sql_function!` / `operator!`, or a typed driver extension.
- **Capability markers are compile-time.** See
  [layering and capability policy](layering.md#capability-policy).
- **Renderer plans are inspectable.** A backend does not have to emulate SQL;
  it receives the neutral plan and decides what to emit.
- **No separate executor layer is planned.** SQLx owns execution; the command
  string is opaque to SQLx.

## Future

- Add a minimal backend example that is not SQL to validate the non-SQL path
  end-to-end.
- Decide whether `SqlWriter` should be split into a language-neutral writer and
  SQL spelling helpers for external backends.
- Provide a reusable driver test suite for capabilities and generated SQL.
