# Postgres example

```bash
cargo run -p wormx-example-postgres
```

The build script creates `public.app_users` and `public.app_todos` in the
development Postgres database (`POSTGRES_URL` or the `DATABASE_URL` from
`.dev.env`). The application in `src/app.rs` demonstrates nested eager loading,
writes, transactions, and SQL extenders (`lower` and `ILIKE`). `main.rs` only
creates the pool and dialect.
