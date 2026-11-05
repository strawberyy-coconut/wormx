# MSSQL example

```bash
cargo run -p wormx-example-mssql
```

The build script connects to `master`, creates `wormx_example` if needed, then
creates `dbo.app_users` and `dbo.app_todos` and writes the schema cache under
`OUT_DIR`.

Set `MSSQL_URL` to use a different server:

```bash
MSSQL_URL='mssql://sa:Password1!@mssql:1433/master?encrypt=on&trust_certificate=true' \
  cargo run -p wormx-example-mssql
```

`src/app.rs` contains the whole application, including a hand-written
`Function<MssqlDialect>` extension plus the `LEN` function and `+` operator
declared with the wormx macros:

```rust
pub struct Upper;

impl wormx::Function<MssqlDialect> for Upper {
    type Args = (wormx::Expr<String, MssqlDialect>,);
    type Output = String;
    const NAME: &'static str = "UPPER";
}
```

`main.rs` only creates the pool and dialect.
