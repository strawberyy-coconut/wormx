//! Microsoft SQL Server support for wormx, built on `sqlx-mssql-rs`.

use wormx_core::{
    Capabilities, Dialect, SqlWriter, SupportsFullTextSearch, SupportsJson, SupportsUpsert,
};

/// The Microsoft SQL Server dialect marker.
#[derive(Debug, Clone, Copy, Default)]
pub struct MssqlDialect;

impl Dialect for MssqlDialect {
    type DB = sqlx_mssql_rs::Mssql;

    type Select = wormx_core::SqlSelect;
    type Insert = wormx_core::SqlInsert;
    type Update = wormx_core::SqlUpdate;
    type Delete = wormx_core::SqlDelete;
    type Upsert = wormx_core::SqlUpsert;

    const CAPABILITIES: Capabilities = Capabilities {
        select: true,
        joins: true,
        group_by: true,
        aggregates: true,
        subqueries: true,
        scan: true,
        returning: false, // MSSQL uses OUTPUT, not RETURNING
        full_text_search: true,
        json: false,
        arrays: false,
        upsert: true,
        generated_columns: true,
    };

    fn quote_ident(&self, name: &str) -> String {
        format!("[{}]", name.replace(']', "]]"))
    }

    fn write_count_star(writer: &mut SqlWriter<Self>) {
        writer.push("COUNT_BIG(*)");
    }

    fn write_count_start(distinct: bool, writer: &mut SqlWriter<Self>) {
        writer.push("COUNT_BIG(");
        if distinct {
            writer.push("DISTINCT ");
        }
    }

    fn write_limit_offset(
        &self,
        has_order: bool,
        limit: Option<u64>,
        offset: Option<u64>,
        out: &mut String,
    ) {
        if limit.is_none() && offset.is_none() {
            return;
        }

        if !has_order {
            out.push_str(" ORDER BY (SELECT NULL)");
        }

        out.push_str(&format!(" OFFSET {} ROWS", offset.unwrap_or(0)));
        if let Some(limit) = limit {
            out.push_str(&format!(" FETCH NEXT {limit} ROWS ONLY"));
        }
    }
}

impl SupportsFullTextSearch for MssqlDialect {}
impl SupportsJson for MssqlDialect {}
impl SupportsUpsert for MssqlDialect {}

#[cfg(feature = "macros")]
mod macros {
    use std::collections::BTreeMap;

    use sqlx_core::connection::Connection;
    use sqlx_core::query::query;
    use sqlx_core::row::Row;
    use sqlx_mssql_rs::{Mssql, MssqlConnection};
    use wormx_core::{Config, TableRef};
    use wormx_macros_core::{Constraints, ForeignKey, IncomingForeignKey, TableDriver};

    pub fn describe(table: &TableRef, database_url: &str) -> Result<serde_json::Value, String> {
        let sql = format!("SELECT * FROM {}", quote_table(table));
        let describe = sqlx_mssql_rs::describe_blocking(&sql, database_url, &Default::default())
            .map_err(|error| error.to_string())?;
        serde_json::to_value(describe).map_err(|error| error.to_string())
    }

    pub fn constraints(
        database_url: &str,
        table: &TableRef,
        _config: &Config,
    ) -> Result<Constraints, Box<dyn std::error::Error + Send + Sync>> {
        block_on(async move {
            let mut connection = MssqlConnection::connect(database_url).await?;
            let object_name = quote_table(table);

            let columns = query(
                r#"
                SELECT
                    c.name AS name,
                    c.is_nullable AS is_nullable,
                    c.is_identity AS is_identity,
                    c.is_computed AS is_computed,
                    d.definition AS default_expr
                FROM sys.columns AS c
                LEFT JOIN sys.default_constraints AS d
                  ON d.parent_object_id = c.object_id
                 AND d.parent_column_id = c.column_id
                WHERE c.object_id = OBJECT_ID(?)
                ORDER BY c.column_id
                "#,
            )
            .bind(&object_name)
            .fetch_all(&mut connection)
            .await?;

            let mut constraints = Constraints::default();
            let mut nullable = BTreeMap::new();

            for row in columns {
                let name: String = row.try_get("name")?;
                nullable.insert(name.clone(), row.try_get::<bool, _>("is_nullable")?);
                if let Some(default_expr) = row.try_get::<Option<String>, _>("default_expr")? {
                    constraints.defaults.insert(name.clone(), default_expr);
                }
                if row.try_get::<bool, _>("is_identity")? {
                    constraints.identity.push(name.clone());
                }
                if row.try_get::<bool, _>("is_computed")? {
                    constraints.generated.push(name);
                }
            }

            let primary_key = query(
                r#"
                SELECT c.name AS name
                FROM sys.indexes AS i
                JOIN sys.index_columns AS ic
                  ON ic.object_id = i.object_id
                 AND ic.index_id = i.index_id
                JOIN sys.columns AS c
                  ON c.object_id = ic.object_id
                 AND c.column_id = ic.column_id
                WHERE i.object_id = OBJECT_ID(?)
                  AND i.is_primary_key = 1
                ORDER BY ic.key_ordinal
                "#,
            )
            .bind(&object_name)
            .fetch_all(&mut connection)
            .await?;

            for row in primary_key {
                constraints.primary_key.push(row.try_get("name")?);
            }

            constraints.foreign_keys =
                foreign_keys(&mut connection, &object_name, false, &nullable).await?;
            constraints.referenced_by =
                incoming_foreign_keys(&mut connection, &object_name).await?;

            let _ = connection.close().await;
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(constraints)
        })
    }

    async fn foreign_keys(
        connection: &mut MssqlConnection,
        object_name: &str,
        incoming: bool,
        _nullable: &BTreeMap<String, bool>,
    ) -> Result<Vec<ForeignKey>, Box<dyn std::error::Error + Send + Sync>> {
        let filter = if incoming {
            "rt.object_id = OBJECT_ID(?)"
        } else {
            "t.object_id = OBJECT_ID(?)"
        };
        let sql = format!(
            r#"
            SELECT
                fk.name AS name,
                c.name AS column_name,
                c.is_nullable AS column_nullable,
                rs.name AS ref_schema,
                rt.name AS ref_table,
                rc.name AS ref_column
            FROM sys.foreign_key_columns AS fkc
            JOIN sys.foreign_keys AS fk
              ON fk.object_id = fkc.constraint_object_id
            JOIN sys.tables AS t
              ON t.object_id = fkc.parent_object_id
            JOIN sys.schemas AS s
              ON s.schema_id = t.schema_id
            JOIN sys.columns AS c
              ON c.object_id = fkc.parent_object_id
             AND c.column_id = fkc.parent_column_id
            JOIN sys.tables AS rt
              ON rt.object_id = fkc.referenced_object_id
            JOIN sys.schemas AS rs
              ON rs.schema_id = rt.schema_id
            JOIN sys.columns AS rc
              ON rc.object_id = fkc.referenced_object_id
             AND rc.column_id = fkc.referenced_column_id
            WHERE {filter}
            ORDER BY fk.name, fkc.constraint_column_id
            "#
        );

        let rows = query(sqlx_core::sql_str::AssertSqlSafe(sql))
            .bind(object_name)
            .fetch_all(connection)
            .await?;

        let mut grouped: BTreeMap<String, ForeignKey> = BTreeMap::new();
        for row in rows {
            let name: String = row.try_get("name")?;
            let entry = grouped.entry(name.clone()).or_insert_with(|| ForeignKey {
                name: Some(name),
                columns: Vec::new(),
                referenced_table: TableRef {
                    schema: row.try_get("ref_schema").ok(),
                    name: row.try_get("ref_table").unwrap_or_default(),
                },
                referenced_columns: Vec::new(),
                columns_nullable: Vec::new(),
            });
            entry.columns.push(row.try_get("column_name")?);
            entry.columns_nullable.push(row.try_get("column_nullable")?);
            entry.referenced_columns.push(row.try_get("ref_column")?);
        }

        Ok(grouped.into_values().collect())
    }

    async fn incoming_foreign_keys(
        connection: &mut MssqlConnection,
        object_name: &str,
    ) -> Result<Vec<IncomingForeignKey>, Box<dyn std::error::Error + Send + Sync>> {
        let sql = r#"
            SELECT
                s.name AS source_schema,
                t.name AS source_table,
                c.name AS column_name,
                c.is_nullable AS column_nullable,
                rc.name AS ref_column
            FROM sys.foreign_key_columns AS fkc
            JOIN sys.foreign_keys AS fk
              ON fk.object_id = fkc.constraint_object_id
            JOIN sys.tables AS t
              ON t.object_id = fkc.parent_object_id
            JOIN sys.schemas AS s
              ON s.schema_id = t.schema_id
            JOIN sys.columns AS c
              ON c.object_id = fkc.parent_object_id
             AND c.column_id = fkc.parent_column_id
            JOIN sys.tables AS rt
              ON rt.object_id = fkc.referenced_object_id
            JOIN sys.columns AS rc
              ON rc.object_id = fkc.referenced_object_id
             AND rc.column_id = fkc.referenced_column_id
            WHERE rt.object_id = OBJECT_ID(?)
            ORDER BY s.name, t.name, fk.name, fkc.constraint_column_id
        "#;

        let rows = query(sqlx_core::sql_str::AssertSqlSafe(sql))
            .bind(object_name)
            .fetch_all(connection)
            .await?;

        let mut grouped: BTreeMap<String, IncomingForeignKey> = BTreeMap::new();
        for row in rows {
            let schema: String = row.try_get("source_schema")?;
            let table: String = row.try_get("source_table")?;
            let key = format!("{schema}.{table}");
            let entry = grouped.entry(key).or_insert_with(|| IncomingForeignKey {
                source_table: TableRef {
                    schema: Some(schema),
                    name: table,
                },
                columns: Vec::new(),
                referenced_columns: Vec::new(),
                columns_nullable: Vec::new(),
            });
            entry.columns.push(row.try_get("column_name")?);
            entry.columns_nullable.push(row.try_get("column_nullable")?);
            entry.referenced_columns.push(row.try_get("ref_column")?);
        }

        Ok(grouped.into_values().collect())
    }

    /// The compile-time table driver for this database.
    pub const MSSQL: TableDriver = TableDriver::new_with_fns::<Mssql>(
        "sqlx_mssql_rs::Mssql",
        &["mssql", "sqlserver", "sql-server"],
        describe,
        constraints,
    );

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to start a Tokio runtime for compile-time schema inspection");
        runtime.block_on(future)
    }

    fn quote_table(table: &TableRef) -> String {
        match &table.schema {
            Some(schema) => format!("{}.{}", quote_ident(schema), quote_ident(&table.name)),
            None => quote_ident(&table.name),
        }
    }

    fn quote_ident(ident: &str) -> String {
        format!("[{}]", ident.replace(']', "]]"))
    }
}

#[cfg(feature = "macros")]
pub use macros::MSSQL;
