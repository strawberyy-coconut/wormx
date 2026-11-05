//! SQLite support for wormx, built on `sqlx-sqlite`.

use wormx_core::{
    Dialect, SqlSyntax, SupportsFullTextSearch, SupportsJson, SupportsReturning, SupportsUpsert,
};

/// The SQLite dialect marker.
#[derive(Debug, Clone, Copy, Default)]
pub struct SqliteDialect;

impl Dialect for SqliteDialect {
    type DB = sqlx_sqlite::Sqlite;

    type Select = wormx_core::SqlSelect;
    type Insert = wormx_core::SqlInsert;
    type Update = wormx_core::SqlUpdate;
    type Delete = wormx_core::SqlDelete;
    type Upsert = wormx_core::SqlUpsert;

    fn quote_ident(&self, name: &str) -> String {
        format!("\"{}\"", name.replace('"', "\"\""))
    }
}

impl SqlSyntax for SqliteDialect {}

impl SupportsReturning for SqliteDialect {}
impl SupportsFullTextSearch for SqliteDialect {}
impl SupportsJson for SqliteDialect {}
impl SupportsUpsert for SqliteDialect {}

#[cfg(feature = "macros")]
mod macros {
    use std::collections::{BTreeMap, HashMap};

    use sqlx_core::connection::Connection;
    use sqlx_core::query::query;
    use sqlx_core::row::Row;
    use sqlx_core::sql_str::AssertSqlSafe;
    use sqlx_sqlite::{Sqlite, SqliteConnection};
    use wormx_core::{Config, TableRef};
    use wormx_macros_core::{Constraints, ForeignKey, IncomingForeignKey, TableDriver};

    pub fn describe(table: &TableRef, database_url: &str) -> Result<serde_json::Value, String> {
        let sql = format!("SELECT * FROM {}", quote_table(table));
        let describe = sqlx_sqlite::describe_blocking(&sql, database_url, &Default::default())
            .map_err(|error| error.to_string())?;
        serde_json::to_value(describe).map_err(|error| error.to_string())
    }

    pub fn constraints(
        database_url: &str,
        table: &TableRef,
        _config: &Config,
    ) -> Result<Constraints, Box<dyn std::error::Error + Send + Sync>> {
        block_on(async move {
            let mut connection = SqliteConnection::connect(database_url).await?;

            let table_name = quote_ident(&table.name);
            let sql = match table.schema.as_deref() {
                Some(schema) if !schema.eq_ignore_ascii_case("main") => {
                    let schema = quote_ident(schema);
                    format!("PRAGMA {schema}.table_xinfo({table_name})")
                }
                _ => format!("PRAGMA table_xinfo({table_name})"),
            };

            let rows = query(AssertSqlSafe(sql)).fetch_all(&mut connection).await?;
            let mut constraints = Constraints::default();
            let mut primary_key = Vec::new();

            for row in rows {
                let name: String = row.try_get("name")?;
                let default_value: Option<String> = row.try_get("dflt_value")?;
                let pk: i64 = row.try_get("pk")?;
                let hidden: i64 = row.try_get("hidden")?;

                if let Some(default_value) = default_value {
                    constraints.defaults.insert(name.clone(), default_value);
                }
                if pk > 0 {
                    primary_key.push((pk, name.clone()));
                }
                if hidden == 2 || hidden == 3 {
                    constraints.generated.push(name);
                }
            }

            primary_key.sort_by_key(|(position, _)| *position);
            constraints.primary_key = primary_key.into_iter().map(|(_, name)| name).collect();

            constraints.foreign_keys = foreign_keys(&mut connection, table).await?;
            constraints.referenced_by = incoming_foreign_keys(&mut connection, table).await?;

            let _ = connection.close().await;
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(constraints)
        })
    }

    async fn foreign_keys(
        connection: &mut SqliteConnection,
        table: &TableRef,
    ) -> Result<Vec<ForeignKey>, Box<dyn std::error::Error + Send + Sync>> {
        let rows = foreign_key_rows(connection, table).await?;
        let nullable = column_nullability(connection, table).await?;
        let mut grouped: BTreeMap<i64, Vec<ForeignKeyRow>> = BTreeMap::new();

        for row in rows {
            grouped.entry(row.id).or_default().push(row);
        }

        let mut result = Vec::new();
        for (id, mut rows) in grouped {
            rows.sort_by_key(|row| row.seq);
            let mut columns = Vec::new();
            let mut referenced_columns = Vec::new();
            let mut columns_nullable = Vec::new();
            let mut referenced_table = None;

            for row in rows {
                columns.push(row.from);
                referenced_columns.push(row.to);
                columns_nullable.push(*nullable.get(columns.last().unwrap()).unwrap_or(&false));
                referenced_table = Some(row.table);
            }

            let Some(referenced_table) = referenced_table else {
                continue;
            };

            result.push(ForeignKey {
                name: Some(format!("fk_{id}")),
                columns,
                referenced_table: TableRef {
                    schema: table.schema.clone(),
                    name: referenced_table,
                },
                referenced_columns,
                columns_nullable,
            });
        }

        Ok(result)
    }

    async fn incoming_foreign_keys(
        connection: &mut SqliteConnection,
        table: &TableRef,
    ) -> Result<Vec<IncomingForeignKey>, Box<dyn std::error::Error + Send + Sync>> {
        let schema = table.schema.as_deref().unwrap_or("main");
        let sql = format!(
            "SELECT name FROM {}.sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
            quote_ident(schema)
        );
        let tables = query(AssertSqlSafe(sql))
            .fetch_all(&mut *connection)
            .await?;

        let mut result = Vec::new();

        for row in tables {
            let source_name: String = row.try_get("name")?;
            let source = TableRef {
                schema: table.schema.clone(),
                name: source_name,
            };
            let rows = foreign_key_rows(connection, &source).await?;
            let nullable = column_nullability(connection, &source).await?;
            let mut grouped: BTreeMap<i64, Vec<ForeignKeyRow>> = BTreeMap::new();

            for row in rows {
                grouped.entry(row.id).or_default().push(row);
            }

            for (_, mut rows) in grouped {
                rows.sort_by_key(|row| row.seq);
                if rows.first().is_none_or(|row| row.table != table.name) {
                    continue;
                }

                let mut columns = Vec::new();
                let mut referenced_columns = Vec::new();
                let mut columns_nullable = Vec::new();

                for row in rows {
                    columns_nullable.push(*nullable.get(&row.from).unwrap_or(&false));
                    columns.push(row.from);
                    referenced_columns.push(row.to);
                }

                result.push(IncomingForeignKey {
                    source_table: source.clone(),
                    columns,
                    referenced_columns,
                    columns_nullable,
                });
            }
        }

        Ok(result)
    }

    struct ForeignKeyRow {
        id: i64,
        seq: i64,
        table: String,
        from: String,
        to: String,
    }

    async fn foreign_key_rows(
        connection: &mut SqliteConnection,
        table: &TableRef,
    ) -> Result<Vec<ForeignKeyRow>, Box<dyn std::error::Error + Send + Sync>> {
        let table_name = quote_ident(&table.name);
        let sql = match table.schema.as_deref() {
            Some(schema) if !schema.eq_ignore_ascii_case("main") => {
                format!(
                    "PRAGMA {}.foreign_key_list({table_name})",
                    quote_ident(schema)
                )
            }
            _ => format!("PRAGMA foreign_key_list({table_name})"),
        };

        let rows = query(AssertSqlSafe(sql))
            .fetch_all(&mut *connection)
            .await?;
        let mut result = Vec::new();

        for row in rows {
            let to: Option<String> = row.try_get("to")?;
            let Some(to) = to else {
                continue;
            };

            result.push(ForeignKeyRow {
                id: row.try_get("id")?,
                seq: row.try_get("seq")?,
                table: row.try_get("table")?,
                from: row.try_get("from")?,
                to,
            });
        }

        Ok(result)
    }

    async fn column_nullability(
        connection: &mut SqliteConnection,
        table: &TableRef,
    ) -> Result<HashMap<String, bool>, Box<dyn std::error::Error + Send + Sync>> {
        let table_name = quote_ident(&table.name);
        let sql = match table.schema.as_deref() {
            Some(schema) if !schema.eq_ignore_ascii_case("main") => {
                format!("PRAGMA {}.table_info({table_name})", quote_ident(schema))
            }
            _ => format!("PRAGMA table_info({table_name})"),
        };

        let rows = query(AssertSqlSafe(sql))
            .fetch_all(&mut *connection)
            .await?;
        let mut result = HashMap::new();

        for row in rows {
            let name: String = row.try_get("name")?;
            let not_null: i64 = row.try_get("notnull")?;
            result.insert(name, not_null == 0);
        }

        Ok(result)
    }

    /// The compile-time table driver for this database.
    pub const SQLITE: TableDriver = TableDriver::new_with_fns::<Sqlite>(
        "::wormx::sqlx::Sqlite",
        &["sqlite", "turso"],
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
pub use macros::SQLITE;
