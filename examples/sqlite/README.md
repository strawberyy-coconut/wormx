# SQLite example

```bash
cargo run -p wormx-example-sqlite
```

The build script creates a SQLite database under `OUT_DIR`. The application in
`src/app.rs` demonstrates nested eager loading, writes, transactions, and SQL
extenders declared with `sql_function!` and `operator!` (`UPPER` and `||`).
`main.rs` only creates the pool and dialect.
