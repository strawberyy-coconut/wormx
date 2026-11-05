//! PostgreSQL support for wormx, built on `sqlx-postgres`.

use wormx_core::{
    Dialect, SqlSyntax, SupportsArrays, SupportsFullTextSearch, SupportsJson, SupportsReturning,
    SupportsUpsert,
};

/// The PostgreSQL dialect marker.
#[derive(Debug, Clone, Copy, Default)]
pub struct PostgresDialect;

impl Dialect for PostgresDialect {
    type DB = sqlx_postgres::Postgres;

    type Select = wormx_core::SqlSelect;
    type Insert = wormx_core::SqlInsert;
    type Update = wormx_core::SqlUpdate;
    type Delete = wormx_core::SqlDelete;
    type Upsert = wormx_core::SqlUpsert;

    fn quote_ident(&self, name: &str) -> String {
        format!("\"{}\"", name.replace('"', "\"\""))
    }
}

impl SqlSyntax for PostgresDialect {}

impl SupportsReturning for PostgresDialect {}
impl SupportsFullTextSearch for PostgresDialect {}
impl SupportsJson for PostgresDialect {}
impl SupportsArrays for PostgresDialect {}
impl SupportsUpsert for PostgresDialect {}

#[cfg(feature = "macros")]
mod macros {
    use sqlx_core::connection::Connection;
    use sqlx_core::executor::Executor;
    use sqlx_core::query::query;
    use sqlx_core::row::Row;
    use sqlx_core::sql_str::{AssertSqlSafe, SqlSafeStr};
    use sqlx_postgres::{PgConnection, Postgres};
    use wormx_core::{Config, TableRef};
    use wormx_macros_core::{Constraints, ForeignKey, IncomingForeignKey, TableDriver};

    pub fn describe(table: &TableRef, database_url: &str) -> Result<serde_json::Value, String> {
        let sql = format!("SELECT * FROM {}", quote_table(table));

        let describe = block_on(async move {
            let mut connection = PgConnection::connect(database_url).await?;
            let sql = AssertSqlSafe(sql).into_sql_str();
            let describe = connection.describe(sql).await?;
            let _ = connection.close().await;
            Ok::<_, sqlx_core::Error>(describe)
        })
        .map_err(|error| error.to_string())?;

        serde_json::to_value(describe).map_err(|error| error.to_string())
    }

    pub fn constraints(
        database_url: &str,
        table: &TableRef,
        _config: &Config,
    ) -> Result<Constraints, Box<dyn std::error::Error + Send + Sync>> {
        block_on(async move {
            let mut connection = PgConnection::connect(database_url).await?;
            let regclass = quote_table(table);

            let columns = query(AssertSqlSafe(
                r#"
                SELECT
                    a.attname::text AS name,
                    pg_get_expr(d.adbin, d.adrelid) AS default_expr,
                    (a.attidentity <> '') AS is_identity,
                    (a.attgenerated <> '') AS is_generated
                FROM pg_attribute a
                JOIN pg_class c ON c.oid = a.attrelid
                LEFT JOIN pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum
                WHERE a.attrelid = $1::regclass
                  AND a.attnum > 0
                  AND NOT a.attisdropped
                ORDER BY a.attnum
                "#,
            ))
            .bind(&regclass)
            .fetch_all(&mut connection)
            .await?;

            let mut constraints = Constraints::default();

            for row in columns {
                let name: String = row.try_get("name")?;
                if let Some(default_expr) = row.try_get::<Option<String>, _>("default_expr")? {
                    constraints.defaults.insert(name.clone(), default_expr);
                }
                if row.try_get::<bool, _>("is_identity")? {
                    constraints.identity.push(name.clone());
                }
                if row.try_get::<bool, _>("is_generated")? {
                    constraints.generated.push(name);
                }
            }

            let primary_key = query(AssertSqlSafe(
                r#"
                SELECT a.attname::text AS name
                FROM pg_index i
                JOIN pg_attribute a
                  ON a.attrelid = i.indrelid
                 AND a.attnum = ANY(i.indkey)
                WHERE i.indrelid = $1::regclass
                  AND i.indisprimary
                ORDER BY array_position(i.indkey, a.attnum)
                "#,
            ))
            .bind(&regclass)
            .fetch_all(&mut connection)
            .await?;

            for row in primary_key {
                constraints.primary_key.push(row.try_get("name")?);
            }

            let foreign_keys = query(AssertSqlSafe(
                r#"
                SELECT
                    con.conname AS name,
                    array_agg(a.attname ORDER BY k.ord) AS columns,
                    array_agg(NOT a.attnotnull ORDER BY k.ord) AS columns_nullable,
                    fn.nspname AS ref_schema,
                    fc.relname AS ref_table,
                    array_agg(fa.attname ORDER BY fk.ord) AS ref_columns
                FROM pg_constraint con
                JOIN pg_class c ON c.oid = con.conrelid
                JOIN pg_class fc ON fc.oid = con.confrelid
                JOIN pg_namespace fn ON fn.oid = fc.relnamespace
                JOIN unnest(con.conkey) WITH ORDINALITY AS k(attnum, ord) ON true
                JOIN pg_attribute a
                  ON a.attrelid = con.conrelid
                 AND a.attnum = k.attnum
                JOIN unnest(con.confkey) WITH ORDINALITY AS fk(attnum, ord)
                  ON fk.ord = k.ord
                JOIN pg_attribute fa
                  ON fa.attrelid = con.confrelid
                 AND fa.attnum = fk.attnum
                WHERE con.contype = 'f'
                  AND con.conrelid = $1::regclass
                GROUP BY con.conname, fn.nspname, fc.relname
                ORDER BY con.conname
                "#,
            ))
            .bind(&regclass)
            .fetch_all(&mut connection)
            .await?;

            for row in foreign_keys {
                constraints.foreign_keys.push(ForeignKey {
                    name: Some(row.try_get("name")?),
                    columns: row.try_get("columns")?,
                    referenced_table: TableRef {
                        schema: row.try_get("ref_schema")?,
                        name: row.try_get("ref_table")?,
                    },
                    referenced_columns: row.try_get("ref_columns")?,
                    columns_nullable: row.try_get("columns_nullable")?,
                });
            }

            let referenced_by = query(AssertSqlSafe(
                r#"
                SELECT
                    n.nspname AS source_schema,
                    c.relname AS source_table,
                    array_agg(a.attname ORDER BY k.ord) AS columns,
                    array_agg(NOT a.attnotnull ORDER BY k.ord) AS columns_nullable,
                    array_agg(fa.attname ORDER BY fk.ord) AS ref_columns
                FROM pg_constraint con
                JOIN pg_class c ON c.oid = con.conrelid
                JOIN pg_namespace n ON n.oid = c.relnamespace
                JOIN pg_class fc ON fc.oid = con.confrelid
                JOIN unnest(con.conkey) WITH ORDINALITY AS k(attnum, ord) ON true
                JOIN pg_attribute a
                  ON a.attrelid = con.conrelid
                 AND a.attnum = k.attnum
                JOIN unnest(con.confkey) WITH ORDINALITY AS fk(attnum, ord)
                  ON fk.ord = k.ord
                JOIN pg_attribute fa
                  ON fa.attrelid = con.confrelid
                 AND fa.attnum = fk.attnum
                WHERE con.contype = 'f'
                  AND con.confrelid = $1::regclass
                GROUP BY n.nspname, c.relname
                ORDER BY n.nspname, c.relname
                "#,
            ))
            .bind(&regclass)
            .fetch_all(&mut connection)
            .await?;

            for row in referenced_by {
                constraints.referenced_by.push(IncomingForeignKey {
                    source_table: TableRef {
                        schema: row.try_get("source_schema")?,
                        name: row.try_get("source_table")?,
                    },
                    columns: row.try_get("columns")?,
                    referenced_columns: row.try_get("ref_columns")?,
                    columns_nullable: row.try_get("columns_nullable")?,
                });
            }

            let _ = connection.close().await;
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(constraints)
        })
    }

    /// The compile-time table driver for this database.
    pub const POSTGRES: TableDriver = TableDriver::new_with_fns::<Postgres>(
        "::wormx::sqlx::Postgres",
        &["postgres", "postgresql"],
        describe,
        constraints,
    );

    fn quote_table(table: &TableRef) -> String {
        match &table.schema {
            Some(schema) => format!("{}.{}", quote_ident(schema), quote_ident(&table.name)),
            None => quote_ident(&table.name),
        }
    }

    fn quote_ident(ident: &str) -> String {
        format!("\"{}\"", ident.replace('"', "\"\""))
    }

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to start a Tokio runtime for compile-time schema inspection");

        runtime.block_on(future)
    }
}

#[cfg(feature = "macros")]
pub use macros::POSTGRES;
