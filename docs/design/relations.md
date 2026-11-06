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

- **Relations are FK-driven.** The generated graph follows the database's
  declared foreign keys; there is no separate relation configuration file.
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
