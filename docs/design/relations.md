# Relations

Relations are generated from foreign keys, not hand-written. The goal is to make
the common foreign-key graph typed while keeping the accessors usable as both a
lazy query source and a loaded value.

## Generation

For:

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

A foreign key on this table generates `belongs_to`; the referenced table gets a
matching `has_many`. The target type is derived from the target table name, or
from `[wormx.tables."..."] struct` / `rust-path`.

## Which relations are generated

The candidate graph comes from foreign keys, but code generation can select a
subset of it. This is useful for incremental adoption, for avoiding tables that
are not generated with `table!`, and for cutting the reverse edges of large
schemas.

```toml
[wormx.codegen]
relations = "auto"

[wormx.tables."public.users"]
relations = "outgoing"
skip-relations = ["audit_logs"]

[wormx.tables."public.posts"]
relations = "explicit"
include-relations = ["blogs", "author"]

[wormx.tables."public.audit_logs"]
relations = "none"
```

The same policy can be set for one `table!` invocation:

```rust
wormx::table! {
    "public.users" as User {
        relations(outgoing),
        include_relations(blogs),
        skip_relations(audit_logs),
    }
}
```

Modes:

| mode | generated relations |
|---|---|
| `auto` | outgoing `belongs_to` and incoming `has_many` |
| `outgoing` | only foreign keys defined by this table |
| `incoming` | only foreign keys that reference this table |
| `explicit` | only names in `include-relations` |
| `none` | no relations |

Resolution order is macro option, then `[wormx.tables."..."]`, then
`[wormx.codegen]`, then `auto`. Include and skip lists are additive across
config and the macro, and skip wins. Names are the generated relation field
names (`blogs`, `user_todos`), and unknown names are errors. `include-relations`
is only valid with `explicit` mode.

A skipped relation is never emitted, so its target type is never referenced.
For example, `table!("public.users")` can be generated without `table!("blogs")`
by skipping the `blogs` relation. `explicit` mode is the typed alternative to
hand-writing a `Deferred` field: the edges are chosen by the user, but the macro
still resolves target types, accessors, and include types.

Relations whose key columns are omitted with `skip-columns` are pruned
automatically. Explicitly including such a relation is an error. See
[Compile-time introspection](compile-time-introspection.md) for column
selection.

Target-side skips are visible when they come from `wormx.toml`. A macro-local
`skip` on the target's `table!` invocation, or a target generated in another
crate with different config, is not visible to the source expansion; keep
relation-key skips in shared config or pair them with a source-side
`skip-relations`.

## Deferred loading

Relation fields are private and stored as `Deferred` values. The relation
accessor is both the query source and the loaded-value accessor:

```rust
let todos = db.query(user.todos()).exec().await?;   // lazy query
let loaded = user.todos().loaded();                 // Option<&Vec<Todos>>
```

For `belongs_to`:

```rust
let owner = db.query(todo.user()).first().await?;   // Option<Users>
let loaded = todo.user().loaded();                  // Option<&Users>
```

`Deferred` distinguishes "not loaded" from "loaded and null", which matters for
nullable foreign keys.

## Eager loading and nesting

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

Includes execute as batched `IN` queries, chunked to stay within bind limits.
`exec_in(&mut tx)` applies includes inside the transaction.

## Nullable and composite keys

Nullable foreign keys generate `Deferred<Option<Target>>` and use
`BelongsTo::optional(...)` / `OptionalBelongsToRef`. A null key produces a
predicate that matches nothing rather than querying with a null argument.

Composite foreign keys generate one relation whose key is a tuple. The row
accessor returns a generated handle with the same `loaded()` / `expect_loaded()`
API and implements `IntoSelect`, so it works with `db.query(...)`. The
corresponding `Fields` accessor supplies the include type used by
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

Multiple foreign keys from the same source table get qualified relation names
such as `user_todos` and `reviewer_todos`.

## Rename-safe loading

Relation loading resolves target fields by their database column name. The
target row type generates hidden per-column accessors, and the relation code
calls them by column name, so renaming a referenced column with `as` does not
break include or eager loading.

A fully hand-written target type would need to provide those hidden accessors
itself. Relation targets are expected to come from `table!`.

## Why many-to-many is not generated

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

## Constraints and decisions

- **Relations are FK-driven but selectable.** The candidate graph follows the
  database's declared foreign keys; `relations`, `include-relations`, and
  `skip-relations` choose which candidates become generated accessors. No
  relation definitions are hand-written; the policy only narrows the
  FK-derived candidates.
- **Skipped key columns prune relations.** Column selection is owned by the
  introspection options; relation code follows the surviving field set.
- **Private fields, public accessors.** Storage is private so the deferred state
  cannot be bypassed; accessors expose query and loaded forms.
- **Batched includes, not N+1 queries.** Includes resolve distinct keys and issue
  chunked `IN` queries.
- **No implicit many-to-many relation.** A junction table is still a normal
  table and composes with the query builder.
- **Database column names are the stable key.** Rust field renames are transparent
  to relation loading.

## Future

- Consider prefetching nested includes in fewer round trips for large graphs.
- Consider configurable relation names for junction tables when a generated
  convenience relation would be clearer than the explicit two-step query.
- Consider a typed batch loader API if applications need to control chunk sizes
  or query shape.
