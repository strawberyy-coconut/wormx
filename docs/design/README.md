# Design notes

These documents capture the constraints, tradeoffs, and future directions behind
wormx. They are for contributors and for users who need to know *why* the API
looks the way it does.

User-facing documentation lives elsewhere:

- the [root README](../../README.md) is the pitch and quick start;
- each crate README covers that crate's public surface and limits;
- the [examples](../../examples) are runnable end-to-end programs.

## Index

- [Layering and capability policy](layering.md) — how `schema`, `plan`, `sql`,
  and `exec` fit together, and when a feature gets a capability marker.
- [Compile-time introspection](compile-time-introspection.md) — the `table!`
  pipeline, `TableDriver`, cache behavior, configuration, and offline builds.
- [Backends and renderers](backends.md) — implementing a dialect, replacing a
  renderer, and extending a backend without changing core.
- [Relations](relations.md) — foreign-key relation generation, deferred loading,
  composite keys, and why many-to-many is not a generated relation.
- [SQL fidelity](sql-fidelity.md) — what the builder renders today, what it
  deliberately does not model, and the next candidates.

## Conventions

Each document separates:

- **Constraints** — deliberate limits that keep the design coherent.
- **Decisions** — tradeoffs that have already been made.
- **Future** — work that is not implemented yet. Nothing in a *Future* section
  should be treated as an API promise.

When adding a feature that changes one of these documents, update the relevant
design note in the same change. If a decision is superseded, keep the history in
the document rather than silently rewriting it.
