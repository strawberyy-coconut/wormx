//! SQLite support for wormx, built on `sqlx-sqlite`.

use wormx_core::{
    Dialect, SqlSyntax, SupportsFullJoin, SupportsFullTextSearch, SupportsJson,
    SupportsNaturalJoin, SupportsReturning, SupportsUpsert, SupportsUsing,
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

impl SqlSyntax for SqliteDialect {
    fn write_limit_offset(
        &self,
        _has_order: bool,
        limit: Option<u64>,
        offset: Option<u64>,
        out: &mut String,
    ) {
        match (limit, offset) {
            (Some(limit), Some(offset)) => {
                out.push_str(&format!(" LIMIT {limit} OFFSET {offset}"));
            }
            (Some(limit), None) => out.push_str(&format!(" LIMIT {limit}")),
            // SQLite requires a LIMIT clause before OFFSET.
            (None, Some(offset)) => out.push_str(&format!(" LIMIT -1 OFFSET {offset}")),
            (None, None) => {}
        }
    }
}

impl SupportsReturning for SqliteDialect {}
impl SupportsFullJoin for SqliteDialect {}
impl SupportsUsing for SqliteDialect {}
impl SupportsNaturalJoin for SqliteDialect {}
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

    type RawForeignKeyRow = (i64, String, String, Option<String>);

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
        let mut grouped: BTreeMap<i64, Vec<RawForeignKeyRow>> = BTreeMap::new();

        for row in rows {
            grouped.entry(row.try_get("id")?).or_default().push((
                row.try_get("seq")?,
                row.try_get("table")?,
                row.try_get("from")?,
                row.try_get("to")?,
            ));
        }

        let mut result = Vec::new();
        let mut primary_keys: HashMap<String, Vec<String>> = HashMap::new();

        for (id, rows) in grouped {
            let Some((_, parent, _, _)) = rows.first() else {
                continue;
            };
            let primary_key = if rows.iter().any(|(_, _, _, to)| to.is_none()) {
                if let Some(columns) = primary_keys.get(parent) {
                    columns.clone()
                } else {
                    let parent = TableRef {
                        schema: table.schema.clone(),
                        name: parent.clone(),
                    };
                    let columns = primary_key_columns(connection, &parent).await?;
                    primary_keys.insert(parent.name, columns.clone());
                    columns
                }
            } else {
                Vec::new()
            };

            for (seq, parent, from, to) in rows {
                let to = match to {
                    Some(column) => column,
                    None => {
                        let position = usize::try_from(seq).map_err(|_| {
                            format!("SQLite foreign key {id} has a negative column position")
                        })?;
                        primary_key.get(position).cloned().ok_or_else(|| {
                            format!(
                                "SQLite foreign key {id} on {table} references {parent} \
                                 without naming a column, but {parent} has no primary-key \
                                 column at position {position}"
                            )
                        })?
                    }
                };

                result.push(ForeignKeyRow {
                    id,
                    seq,
                    table: parent,
                    from,
                    to,
                });
            }
        }

        Ok(result)
    }

    async fn primary_key_columns(
        connection: &mut SqliteConnection,
        table: &TableRef,
    ) -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
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
        let mut primary_key = Vec::new();

        for row in rows {
            let position: i64 = row.try_get("pk")?;
            if position > 0 {
                primary_key.push((position, row.try_get::<String, _>("name")?));
            }
        }

        primary_key.sort_by_key(|(position, _)| *position);
        Ok(primary_key.into_iter().map(|(_, name)| name).collect())
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
    pub const SQLITE: TableDriver =
        TableDriver::new_with_fns::<Sqlite>(&["sqlite", "turso"], describe, constraints);

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
