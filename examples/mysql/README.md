# MySQL example

This example is standalone so the root workspace can build without a running
MySQL server.

Start the development MySQL service:

```bash
docker compose -f ../../compose.dev.yaml up -d mysql
```

Then run it with the default URL (inside the compose network):

```bash
cargo run
```

or override it:

```bash
MYSQL_URL='mysql://root:Password1!@127.0.0.1:3306/wormx_example' cargo run
```

The build script creates `app_users` and `app_todos` in that database and
generates the compile-time schema cache. `src/app.rs` demonstrates nested eager
loading, writes, transactions, and MySQL SQL extenders (`UPPER`, `CONCAT`, and
the null-safe `<=>` operator). `main.rs` only creates the pool and dialect.
