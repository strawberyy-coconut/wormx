//! MySQL support for wormx, built on `sqlx-mysql`.

use sqlx_core::error::{Error, Result};
use wormx_core::{
    Dialect, SqlSyntax, SqlWriter, SupportsFullTextSearch, SupportsJson, SupportsUpsert,
    UpsertPlan, UpsertRenderer,
};

/// The MySQL dialect marker.
#[derive(Debug, Clone, Copy, Default)]
pub struct MySqlDialect;

impl Dialect for MySqlDialect {
    type DB = sqlx_mysql::MySql;

    type Select = wormx_core::SqlSelect;
    type Insert = wormx_core::SqlInsert;
    type Update = wormx_core::SqlUpdate;
    type Delete = wormx_core::SqlDelete;
    type Upsert = MySqlUpsert;

    fn quote_ident(&self, name: &str) -> String {
        format!("`{}`", name.replace('`', "``"))
    }
}

impl SqlSyntax for MySqlDialect {
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
            // MySQL requires LIMIT when OFFSET is present.
            (None, Some(offset)) => {
                out.push_str(&format!(" LIMIT 18446744073709551615 OFFSET {offset}"));
            }
            (None, None) => {}
        }
    }

    fn write_insert_default(writer: &mut SqlWriter<Self>) {
        writer.push(" () VALUES ()");
    }
}

/// MySQL upsert renderer using `INSERT ... ON DUPLICATE KEY UPDATE`.
#[derive(Debug, Clone, Copy, Default)]
pub struct MySqlUpsert;

impl UpsertRenderer<MySqlDialect> for MySqlUpsert {
    fn write(
        &self,
        plan: UpsertPlan<'_, MySqlDialect>,
        writer: &mut SqlWriter<MySqlDialect>,
    ) -> Result<()> {
        if plan.assignments.is_empty() {
            return Err(Error::InvalidArgument(
                "UPSERT requires at least one .update(...) assignment for MySQL".to_owned(),
            ));
        }

        writer.write_insert();
        push_table(writer, plan.table.def);

        writer.write_group_start();
        for (index, column) in plan.columns.iter().enumerate() {
            if index > 0 {
                writer.write_comma();
            }
            writer.push_ident(column);
        }
        writer.write_group_end();
        writer.write_values();

        for (index, value) in plan.values.iter_mut().enumerate() {
            if index > 0 {
                writer.write_comma();
            }
            value.render(writer)?;
        }
        writer.write_group_end();

        writer.push(" ON DUPLICATE KEY UPDATE ");
        for (index, (column, value)) in plan.assignments.iter_mut().enumerate() {
            if index > 0 {
                writer.write_comma();
            }
            writer.push_ident(column);
            writer.write_equals();
            value.render(writer)?;
        }

        Ok(())
    }
}

fn push_table(writer: &mut SqlWriter<MySqlDialect>, table: &wormx_core::TableDef) {
    if let Some(schema) = table.schema {
        writer.push_ident(schema);
        writer.write_identifier_separator();
    }
    writer.push_ident(table.name);
}

impl SupportsFullTextSearch for MySqlDialect {}
impl SupportsJson for MySqlDialect {}
impl SupportsUpsert for MySqlDialect {}

#[cfg(feature = "macros")]
mod macros {
    use std::collections::BTreeMap;

    use sqlx_core::connection::Connection;
    use sqlx_core::executor::Executor;
    use sqlx_core::query::query;
    use sqlx_core::row::Row;
    use sqlx_core::sql_str::{AssertSqlSafe, SqlSafeStr};
    use sqlx_mysql::{MySql, MySqlConnection};
    use wormx_core::{Config, TableRef};
    use wormx_macros_core::{Constraints, ForeignKey, IncomingForeignKey, TableDriver};

    pub fn describe(table: &TableRef, database_url: &str) -> Result<serde_json::Value, String> {
        let sql = format!("SELECT * FROM {}", quote_table(table));

        let describe = block_on(async move {
            let mut connection = MySqlConnection::connect(database_url).await?;
            let describe = connection
                .describe(AssertSqlSafe(sql).into_sql_str())
                .await?;
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
            let mut connection = MySqlConnection::connect(database_url).await?;
            let schema = match &table.schema {
                Some(schema) => schema.clone(),
                None => {
                    let row = query("SELECT DATABASE() AS name")
                        .fetch_one(&mut connection)
                        .await?;
                    row.try_get::<String, _>("name")?
                }
            };

            let mut constraints = Constraints::default();

            let columns = query(AssertSqlSafe(
                r#"
                SELECT COLUMN_NAME AS name, COLUMN_DEFAULT AS default_expr, EXTRA AS extra
                FROM information_schema.COLUMNS
                WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ?
                ORDER BY ORDINAL_POSITION
                "#,
            ))
            .bind(&schema)
            .bind(&table.name)
            .fetch_all(&mut connection)
            .await?;

            for row in columns {
                let name: String = row.try_get("name")?;
                if let Some(default_expr) = row.try_get::<Option<String>, _>("default_expr")? {
                    constraints.defaults.insert(name.clone(), default_expr);
                }
                let extra: String = row.try_get("extra")?;
                let extra = extra.to_ascii_lowercase();
                if extra.contains("auto_increment") {
                    constraints.identity.push(name.clone());
                }
                if extra.contains("virtual generated") || extra.contains("stored generated") {
                    constraints.generated.push(name);
                }
            }

            let primary_key = query(AssertSqlSafe(
                r#"
                SELECT COLUMN_NAME AS name
                FROM information_schema.KEY_COLUMN_USAGE
                WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? AND CONSTRAINT_NAME = 'PRIMARY'
                ORDER BY ORDINAL_POSITION
                "#,
            ))
            .bind(&schema)
            .bind(&table.name)
            .fetch_all(&mut connection)
            .await?;

            for row in primary_key {
                constraints.primary_key.push(row.try_get("name")?);
            }

            let foreign_keys = query(AssertSqlSafe(
                r#"
                SELECT
                    k.CONSTRAINT_NAME AS name,
                    k.COLUMN_NAME AS column_name,
                    k.REFERENCED_TABLE_SCHEMA AS ref_schema,
                    k.REFERENCED_TABLE_NAME AS ref_table,
                    k.REFERENCED_COLUMN_NAME AS ref_column,
                    c.IS_NULLABLE AS nullable
                FROM information_schema.KEY_COLUMN_USAGE k
                JOIN information_schema.COLUMNS c
                  ON c.TABLE_SCHEMA = k.TABLE_SCHEMA
                 AND c.TABLE_NAME = k.TABLE_NAME
                 AND c.COLUMN_NAME = k.COLUMN_NAME
                WHERE k.TABLE_SCHEMA = ? AND k.TABLE_NAME = ?
                  AND k.REFERENCED_TABLE_NAME IS NOT NULL
                ORDER BY k.CONSTRAINT_NAME, k.ORDINAL_POSITION
                "#,
            ))
            .bind(&schema)
            .bind(&table.name)
            .fetch_all(&mut connection)
            .await?;

            let mut grouped: BTreeMap<String, ForeignKey> = BTreeMap::new();
            for row in foreign_keys {
                let name: String = row.try_get("name")?;
                let entry = grouped.entry(name.clone()).or_insert_with(|| ForeignKey {
                    name: Some(name),
                    columns: Vec::new(),
                    referenced_table: TableRef {
                        schema: row
                            .try_get::<Option<String>, _>("ref_schema")
                            .unwrap_or(None),
                        name: row.try_get("ref_table").unwrap_or_default(),
                    },
                    referenced_columns: Vec::new(),
                    columns_nullable: Vec::new(),
                });
                entry.columns.push(row.try_get("column_name")?);
                entry.referenced_columns.push(row.try_get("ref_column")?);
                entry.columns_nullable.push(
                    row.try_get::<String, _>("nullable")
                        .map(|value| value.eq_ignore_ascii_case("YES"))
                        .unwrap_or(false),
                );
            }
            constraints.foreign_keys = grouped.into_values().collect();

            let referenced_by = query(AssertSqlSafe(
                r#"
                SELECT
                    k.CONSTRAINT_NAME AS name,
                    k.TABLE_SCHEMA AS source_schema,
                    k.TABLE_NAME AS source_table,
                    k.COLUMN_NAME AS column_name,
                    k.REFERENCED_COLUMN_NAME AS ref_column,
                    c.IS_NULLABLE AS nullable
                FROM information_schema.KEY_COLUMN_USAGE k
                JOIN information_schema.COLUMNS c
                  ON c.TABLE_SCHEMA = k.TABLE_SCHEMA
                 AND c.TABLE_NAME = k.TABLE_NAME
                 AND c.COLUMN_NAME = k.COLUMN_NAME
                WHERE k.REFERENCED_TABLE_SCHEMA = ? AND k.REFERENCED_TABLE_NAME = ?
                  AND k.REFERENCED_TABLE_NAME IS NOT NULL
                ORDER BY k.TABLE_SCHEMA, k.TABLE_NAME, k.CONSTRAINT_NAME, k.ORDINAL_POSITION
                "#,
            ))
            .bind(&schema)
            .bind(&table.name)
            .fetch_all(&mut connection)
            .await?;

            let mut grouped: BTreeMap<(String, String, String), IncomingForeignKey> =
                BTreeMap::new();
            for row in referenced_by {
                let key = (
                    row.try_get::<String, _>("source_schema")?,
                    row.try_get::<String, _>("source_table")?,
                    row.try_get::<String, _>("name")?,
                );
                let entry = grouped.entry(key).or_insert_with(|| IncomingForeignKey {
                    source_table: TableRef {
                        schema: None,
                        name: String::new(),
                    },
                    columns: Vec::new(),
                    referenced_columns: Vec::new(),
                    columns_nullable: Vec::new(),
                });
                let source_schema: String = row.try_get("source_schema")?;
                entry.source_table.schema = Some(source_schema);
                entry.source_table.name = row.try_get("source_table")?;
                entry.columns.push(row.try_get("column_name")?);
                entry.referenced_columns.push(row.try_get("ref_column")?);
                entry.columns_nullable.push(
                    row.try_get::<String, _>("nullable")
                        .map(|value| value.eq_ignore_ascii_case("YES"))
                        .unwrap_or(false),
                );
            }
            constraints.referenced_by = grouped.into_values().collect();

            let _ = connection.close().await;
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(constraints)
        })
    }

    /// The compile-time table driver for this database.
    pub const MYSQL: TableDriver =
        TableDriver::new_with_fns::<MySql>(&["mysql", "mariadb"], describe, constraints);

    fn quote_table(table: &TableRef) -> String {
        match &table.schema {
            Some(schema) => format!("{}.{}", quote_ident(schema), quote_ident(&table.name)),
            None => quote_ident(&table.name),
        }
    }

    fn quote_ident(ident: &str) -> String {
        format!("`{}`", ident.replace('`', "``"))
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
pub use macros::MYSQL;

#[cfg(test)]
mod tests {
    use wormx_core::{SqlSyntax, SqlWriter};

    use super::MySqlDialect;

    #[test]
    fn empty_insert_uses_mysql_syntax() {
        let mut writer = SqlWriter::new(MySqlDialect);
        <MySqlDialect as SqlSyntax>::write_insert_default(&mut writer);
        let (sql, _) = writer.finish();
        assert_eq!(sql, " () VALUES ()");
    }

    #[test]
    fn offset_without_limit_adds_a_limit() {
        let mut out = String::new();
        MySqlDialect.write_limit_offset(false, None, Some(5), &mut out);
        assert_eq!(out, " LIMIT 18446744073709551615 OFFSET 5");
    }

    #[test]
    fn limit_and_offset_use_mysql_order() {
        let mut out = String::new();
        MySqlDialect.write_limit_offset(false, Some(10), Some(5), &mut out);
        assert_eq!(out, " LIMIT 10 OFFSET 5");
    }
}
