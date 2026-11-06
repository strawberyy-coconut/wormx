# SQL fidelity

The builder should make the common path typed without pretending to be a
complete SQL parser. This document records what is rendered today and what is
deliberately left to raw SQL.

## Supported directly

- `SELECT` projections, including tuples of up to 16 elements, computed
  expressions, and aggregates
- `DISTINCT`
- `WHERE` with SQL precedence preserved by explicit grouping
- `IN`, `NOT IN`, `IN (subquery)`
- `JOIN` / `LEFT JOIN` / `RIGHT JOIN` / `FULL JOIN` / `CROSS JOIN`
- `JOIN ... USING (...)` and `NATURAL JOIN` on dialects implementing
  `SupportsUsing` / `SupportsNaturalJoin`; SQL Server does not support either
  (`FULL JOIN` is similarly gated by `SupportsFullJoin`; MySQL and MariaDB do
  not support it)
- `GROUP BY` and `HAVING`
- `ORDER BY`, `LIMIT`, `OFFSET`
- `COUNT(*)`, `COUNT(expr)`, `COUNT(DISTINCT expr)` (SQL Server renders these
  as `COUNT_BIG`)
- `CASE WHEN ... THEN ... ELSE ... END` and `CAST(expr AS type)`
- CTEs (`.with(...)`) and window functions (`.over(...)`)
- insert, update, delete, and upsert builders
- multi-row `INSERT` and `UPSERT`, plus per-row `UPDATE ... CASE` batches
- `RETURNING` on dialects that support it (`.returning().exec()`)
- driver-specific `LIMIT`/`OFFSET` spelling, plus opaque source suffixes and
  tails for dialect extensions such as SQL Server hints and `OPTION (...)`
- explicit grouping for custom operators, plus `Expr::group()`

## Not modeled yet

- `UNION` / `INTERSECT` / `EXCEPT`
- `EXISTS` / `NOT EXISTS`
- column aliases and ordered null placement

Those are the next places where the builder stops simulating SQL and starts
hiding it. Raw SQL exists so applications are not blocked in the meantime.

## Constraints

- **Raw SQL is the escape hatch.** `db.sql(...).bind(...)` is part of the design,
  not a failure of the DSL.
- **Explicit grouping is intentional.** The builder does not try to infer user
  precedence for custom operators; `grouping = Operands` is the safe default.
- **`Expr::raw` does not bind.** Never interpolate user input into `raw`; use
  `.eq(value)` or `Expr::bind(value)` instead.
- **Type names in `cast` are verbatim.** `cast("NVARCHAR(200)")` must be a
  constant, never user input.
- **Rows are not validated against SQL type declarations.** `sql_function!` and
  `operator!` check arity, not the database's result type.
- **Batch statements are single statements.** Keep batches within the database's
  bind-parameter limit.
- **`update_many` has one key column.** That is a current shape constraint, not a
  SQL limitation.

## Future

- `UNION`, `INTERSECT`, and `EXCEPT`
- `EXISTS` / `NOT EXISTS`
- projected column aliases
- ordered null placement (`NULLS FIRST` / `NULLS LAST`)
- multi-column keys for per-row batch updates
- richer aggregate and window helpers if they can remain backend-neutral

These are candidates, not commitments. Add a capability marker if an operation
cannot be rendered by every bundled backend.
