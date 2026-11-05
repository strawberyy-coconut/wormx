//! Microsoft SQL Server support for wormx, built on `sqlx-mssql-rs`.

use sqlx_core::error::{Error, Result};
use wormx_core::{
    Delete, Dialect, ProjectedSelect, Projection, ReturningOp, ReturningPosition, Select,
    SqlSyntax, SqlWriter, SupportsFullJoin, SupportsFullTextSearch, SupportsReturning,
    SupportsUpsert, Table, TableSource, Update, Upsert, UpsertPlan, UpsertRenderer,
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
    type Upsert = MssqlUpsert;

    fn quote_ident(&self, name: &str) -> String {
        format!("[{}]", name.replace(']', "]]"))
    }
}

impl SqlSyntax for MssqlDialect {
    fn write_count_star(writer: &mut SqlWriter<Self>) {
        writer.push("COUNT_BIG(*)");
    }

    /// SQL Server spells the count function `COUNT_BIG`; the renderer adds the
    /// opening parenthesis and the `DISTINCT` keyword.
    fn write_count_start(_distinct: bool, writer: &mut SqlWriter<Self>) {
        writer.push("COUNT_BIG");
    }

    fn write_source_suffix(&self, suffix: &str, writer: &mut SqlWriter<Self>) {
        writer.push(" WITH (");
        writer.push(suffix);
        writer.push(")");
    }

    fn write_tail(&self, tail: &str, writer: &mut SqlWriter<Self>) {
        writer.push(" OPTION (");
        writer.push(tail);
        writer.push(")");
    }

    fn returning_position(&self, op: ReturningOp) -> ReturningPosition {
        match op {
            ReturningOp::Insert => ReturningPosition::BeforeValues,
            ReturningOp::Update | ReturningOp::Delete => ReturningPosition::BeforeWhere,
            ReturningOp::Upsert => ReturningPosition::Trailing,
        }
    }

    fn write_returning_clause(&self, op: ReturningOp, writer: &mut SqlWriter<Self>) {
        match op {
            ReturningOp::Delete => writer.push(" OUTPUT DELETED.*"),
            ReturningOp::Insert | ReturningOp::Update | ReturningOp::Upsert => {
                writer.push(" OUTPUT INSERTED.*");
            }
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

/// A typed SQL Server table hint.
///
/// Use [`TableHint::Raw`] for hints not modeled here, or to combine several
/// hints in one string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableHint {
    NoLock,
    RowLock,
    ReadPast,
    UpdLock,
    HoldLock,
    TabLock,
    TabLockX,
    RepeatableRead,
    Serializable,
    Raw(&'static str),
}

impl TableHint {
    /// The SQL text of the hint, without a `WITH (...)` wrapper.
    pub const fn as_sql(self) -> &'static str {
        match self {
            Self::NoLock => "NOLOCK",
            Self::RowLock => "ROWLOCK",
            Self::ReadPast => "READPAST",
            Self::UpdLock => "UPDLOCK",
            Self::HoldLock => "HOLDLOCK",
            Self::TabLock => "TABLOCK",
            Self::TabLockX => "TABLOCKX",
            Self::RepeatableRead => "REPEATABLEREAD",
            Self::Serializable => "SERIALIZABLE",
            Self::Raw(sql) => sql,
        }
    }
}

/// Converts a raw string or [`TableHint`] into SQL Server hint text.
pub trait IntoTableHint {
    fn into_table_hint(self) -> &'static str;
}

impl IntoTableHint for &'static str {
    fn into_table_hint(self) -> &'static str {
        self
    }
}

impl IntoTableHint for TableHint {
    fn into_table_hint(self) -> &'static str {
        self.as_sql()
    }
}

/// SQL Server table-hint builders.
///
/// The core `wormx` builders only know about opaque source suffixes. This trait
/// gives them SQL Server vocabulary without putting `ROWLOCK`, `READPAST`, or
/// the `WITH (...)` syntax in core.
pub trait MssqlTableExt: Sized {
    /// Attaches a SQL Server table hint, for example `TableHint::RowLock` or
    /// `"ROWLOCK, READPAST"`.
    fn hint<H: IntoTableHint>(self, hint: H) -> Self;
}

impl MssqlTableExt for TableSource {
    fn hint<H: IntoTableHint>(self, hint: H) -> Self {
        self.with_suffix(hint.into_table_hint())
    }
}

impl<T: Table> MssqlTableExt for Select<MssqlDialect, T> {
    fn hint<H: IntoTableHint>(self, hint: H) -> Self {
        self.with_source_suffix(hint.into_table_hint())
    }
}

impl<T, P> MssqlTableExt for ProjectedSelect<MssqlDialect, T, P>
where
    T: Table,
    P: Projection<MssqlDialect>,
{
    fn hint<H: IntoTableHint>(self, hint: H) -> Self {
        self.with_source_suffix(hint.into_table_hint())
    }
}

impl<T: Table> MssqlTableExt for Update<MssqlDialect, T> {
    fn hint<H: IntoTableHint>(self, hint: H) -> Self {
        self.with_source_suffix(hint.into_table_hint())
    }
}

impl<T: Table> MssqlTableExt for Delete<MssqlDialect, T> {
    fn hint<H: IntoTableHint>(self, hint: H) -> Self {
        self.with_source_suffix(hint.into_table_hint())
    }
}

impl<T: Table> MssqlTableExt for Upsert<MssqlDialect, T> {
    fn hint<H: IntoTableHint>(self, hint: H) -> Self {
        self.with_source_suffix(hint.into_table_hint())
    }
}

/// SQL Server statement-tail builders.
pub trait MssqlStatementExt: Sized {
    /// Sets the body of a SQL Server `OPTION (...)` clause, for example
    /// `"RECOMPILE"`.
    fn option(self, options: &'static str) -> Self;
}

impl<T: Table> MssqlStatementExt for Select<MssqlDialect, T> {
    fn option(self, options: &'static str) -> Self {
        self.with_tail(options)
    }
}

impl<T, P> MssqlStatementExt for ProjectedSelect<MssqlDialect, T, P>
where
    T: Table,
    P: Projection<MssqlDialect>,
{
    fn option(self, options: &'static str) -> Self {
        self.with_tail(options)
    }
}

/// SQL Server upsert renderer using `MERGE ... WITH (HOLDLOCK)`.
///
/// `MERGE` has no `ON CONFLICT`/`DO UPDATE` spelling, so this renderer builds a
/// single `MERGE` statement from the insert values and the update assignments.
#[derive(Debug, Clone, Copy, Default)]
pub struct MssqlUpsert;

impl UpsertRenderer<MssqlDialect> for MssqlUpsert {
    fn write(
        &self,
        plan: UpsertPlan<'_, MssqlDialect>,
        writer: &mut SqlWriter<MssqlDialect>,
    ) -> Result<()> {
        if plan.conflict_columns.is_empty() {
            return Err(Error::InvalidArgument(
                "UPSERT requires at least one conflict column; call .conflict_on(...)".to_owned(),
            ));
        }

        writer.push("MERGE INTO ");
        push_table(writer, plan.table.def);

        // HOLDLOCK makes the MERGE safe under concurrency. If the caller also
        // supplied hints, keep them without duplicating HOLDLOCK.
        writer.push(" WITH (HOLDLOCK");
        if let Some(suffix) = plan.table.suffix
            && !suffix.trim().is_empty()
            && !suffix.to_ascii_uppercase().contains("HOLDLOCK")
        {
            writer.push(", ");
            writer.push(suffix);
        }
        writer.push(") AS ");
        writer.push_ident("target");

        writer.push(" USING (SELECT ");
        for (index, (column, value)) in plan.columns.iter().zip(plan.values.iter_mut()).enumerate()
        {
            if index > 0 {
                writer.write_comma();
            }
            value.render(writer)?;
            writer.write_as();
            writer.push_ident(column);
        }
        writer.push(") AS ");
        writer.push_ident("source");

        writer.push(" ON ");
        for (index, column) in plan.conflict_columns.iter().enumerate() {
            if index > 0 {
                writer.push(" AND ");
            }
            push_qualified(writer, "target", column);
            writer.write_equals();
            push_qualified(writer, "source", column);
        }

        if !plan.assignments.is_empty() {
            writer.push(" WHEN MATCHED THEN UPDATE SET ");
            for (index, (column, value)) in plan.assignments.iter_mut().enumerate() {
                if index > 0 {
                    writer.write_comma();
                }
                writer.push_ident(column);
                writer.write_equals();
                value.render(writer)?;
            }
        }

        writer.push(" WHEN NOT MATCHED THEN INSERT (");
        for (index, column) in plan.columns.iter().enumerate() {
            if index > 0 {
                writer.write_comma();
            }
            writer.push_ident(column);
        }
        writer.push(") VALUES (");
        for (index, column) in plan.columns.iter().enumerate() {
            if index > 0 {
                writer.write_comma();
            }
            push_qualified(writer, "source", column);
        }
        writer.push(")");

        if plan.returning {
            writer.write_returning_clause(ReturningOp::Upsert);
        }
        writer.push(";");

        Ok(())
    }
}

fn push_table(writer: &mut SqlWriter<MssqlDialect>, table: &wormx_core::TableDef) {
    if let Some(schema) = table.schema {
        writer.push_ident(schema);
        writer.write_identifier_separator();
    }
    writer.push_ident(table.name);
}

fn push_qualified(writer: &mut SqlWriter<MssqlDialect>, alias: &str, column: &str) {
    writer.push_ident(alias);
    writer.write_identifier_separator();
    writer.push_ident(column);
}

impl SupportsFullJoin for MssqlDialect {}
impl SupportsFullTextSearch for MssqlDialect {}
impl SupportsReturning for MssqlDialect {}
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
                fk.name AS name,
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

        let mut grouped: BTreeMap<(String, String, String), IncomingForeignKey> = BTreeMap::new();
        for row in rows {
            let schema: String = row.try_get("source_schema")?;
            let table: String = row.try_get("source_table")?;
            let name: String = row.try_get("name")?;
            let key = (schema.clone(), table.clone(), name);
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

#[cfg(test)]
mod tests {
    use super::*;
    use wormx_core::{
        ColumnDef, DeletePlan, DeleteRenderer, Expr, InsertPlan, InsertRenderer, SelectRenderer,
        SqlDelete, SqlInsert, SqlSelect, SqlUpdate, StarProjection, TableDef, TableSource,
        UpdatePlan, UpdateRenderer, UpsertRenderer,
    };

    static COLUMNS: &[ColumnDef] = &[
        ColumnDef {
            name: "id",
            sql_type: "bigint",
            nullable: false,
            primary_key: true,
            has_default: false,
            identity: true,
            generated: false,
        },
        ColumnDef {
            name: "name",
            sql_type: "nvarchar",
            nullable: false,
            primary_key: false,
            has_default: false,
            identity: false,
            generated: false,
        },
    ];
    static PRIMARY_KEY: &[&str] = &["id"];
    static TABLE: TableDef = TableDef {
        name: "wormx_users",
        schema: Some("dbo"),
        columns: COLUMNS,
        primary_key: PRIMARY_KEY,
        relations: &[],
    };

    fn upsert_plan<'a>(
        values: &'a mut [Expr<(), MssqlDialect>],
        assignments: &'a mut [(&'static str, Expr<(), MssqlDialect>)],
        columns: &'a [&'static str],
        conflict_columns: &'a [&'static str],
        returning: bool,
    ) -> UpsertPlan<'a, MssqlDialect> {
        UpsertPlan {
            table: TableSource::new(&TABLE, None),
            columns,
            values,
            conflict_columns,
            assignments,
            returning,
        }
    }

    #[test]
    fn renders_merge_upsert() {
        let columns = ["id", "name"];
        let conflict_columns = ["id"];
        let mut values = vec![
            Expr::<(), MssqlDialect>::raw("?"),
            Expr::<(), MssqlDialect>::raw("?"),
        ];
        let mut assignments = vec![("name", Expr::<(), MssqlDialect>::raw("?"))];
        let plan = upsert_plan(
            &mut values,
            &mut assignments,
            &columns,
            &conflict_columns,
            false,
        );

        let mut writer = SqlWriter::new(MssqlDialect);
        MssqlUpsert.write(plan, &mut writer).expect("render merge");
        let (sql, _args) = writer.finish();

        assert!(
            sql.starts_with(
                "MERGE INTO [dbo].[wormx_users] WITH (HOLDLOCK) AS [target] USING (SELECT ? AS [id], ? AS [name]) AS [source]"
            ),
            "{sql}"
        );
        assert!(sql.contains("ON [target].[id] = [source].[id]"), "{sql}");
        assert!(
            sql.contains("WHEN MATCHED THEN UPDATE SET [name] = ?"),
            "{sql}"
        );
        assert!(
            sql.ends_with(
                "WHEN NOT MATCHED THEN INSERT ([id], [name]) VALUES ([source].[id], [source].[name]);"
            ),
            "{sql}"
        );
    }

    #[test]
    fn merge_upsert_emits_output_when_returning() {
        let columns = ["id", "name"];
        let conflict_columns = ["id"];
        let mut values = vec![
            Expr::<(), MssqlDialect>::raw("?"),
            Expr::<(), MssqlDialect>::raw("?"),
        ];
        let mut assignments = vec![("name", Expr::<(), MssqlDialect>::raw("?"))];
        let plan = upsert_plan(
            &mut values,
            &mut assignments,
            &columns,
            &conflict_columns,
            true,
        );

        let mut writer = SqlWriter::new(MssqlDialect);
        MssqlUpsert.write(plan, &mut writer).expect("render merge");
        let (sql, _args) = writer.finish();

        assert!(sql.ends_with("OUTPUT INSERTED.*;"), "{sql}");
        assert!(!sql.contains("RETURNING"), "{sql}");
    }

    #[test]
    fn renders_output_for_returning_dml() {
        let columns = ["name"];
        let mut values = vec![Expr::<(), MssqlDialect>::raw("?")];
        let mut writer = SqlWriter::new(MssqlDialect);
        SqlInsert
            .write(
                InsertPlan {
                    table: &TABLE,
                    columns: &columns,
                    values: &mut values,
                    returning: true,
                },
                &mut writer,
            )
            .expect("render insert");
        let (sql, _args) = writer.finish();
        assert_eq!(
            sql,
            "INSERT INTO [dbo].[wormx_users]([name]) OUTPUT INSERTED.* VALUES (?)"
        );

        let mut writer = SqlWriter::new(MssqlDialect);
        SqlInsert
            .write(
                InsertPlan {
                    table: &TABLE,
                    columns: &[],
                    values: &mut [],
                    returning: true,
                },
                &mut writer,
            )
            .expect("render default insert");
        let (sql, _args) = writer.finish();
        assert_eq!(
            sql,
            "INSERT INTO [dbo].[wormx_users] OUTPUT INSERTED.* DEFAULT VALUES"
        );

        let mut assignments = vec![("name", Expr::<(), MssqlDialect>::raw("?"))];
        let mut filters = vec![Expr::<bool, MssqlDialect>::raw("1 = 1")];
        let mut writer = SqlWriter::new(MssqlDialect);
        SqlUpdate
            .write(
                UpdatePlan {
                    table: TableSource::new(&TABLE, None),
                    assignments: &mut assignments,
                    filters: &mut filters,
                    returning: true,
                },
                &mut writer,
            )
            .expect("render update");
        let (sql, _args) = writer.finish();
        assert_eq!(
            sql,
            "UPDATE [dbo].[wormx_users] SET [name] = ? OUTPUT INSERTED.* WHERE (1 = 1)"
        );

        let mut filters = vec![Expr::<bool, MssqlDialect>::raw("1 = 1")];
        let mut writer = SqlWriter::new(MssqlDialect);
        SqlDelete
            .write(
                DeletePlan {
                    table: TableSource::new(&TABLE, None),
                    filters: &mut filters,
                    returning: true,
                },
                &mut writer,
            )
            .expect("render delete");
        let (sql, _args) = writer.finish();
        assert_eq!(
            sql,
            "DELETE FROM [dbo].[wormx_users] OUTPUT DELETED.* WHERE (1 = 1)"
        );
    }

    #[test]
    fn renders_table_hints_and_options() {
        let mut writer = SqlWriter::new(MssqlDialect);
        let mut projection = StarProjection;
        let mut filters = Vec::new();
        let mut joins = Vec::new();
        let mut groups = Vec::new();
        let mut having = Vec::new();
        let mut orders = Vec::new();

        SqlSelect
            .write(
                wormx_core::SelectPlan {
                    table: TableSource::new(&TABLE, None).with_suffix("ROWLOCK, READPAST"),
                    distinct: false,
                    projection: &mut projection,
                    ctes: &mut [],
                    filters: &mut filters,
                    joins: &mut joins,
                    groups: &mut groups,
                    having: &mut having,
                    orders: &mut orders,
                    limit: None,
                    offset: None,
                    tail: Some("RECOMPILE"),
                },
                &mut writer,
            )
            .expect("render select");
        let (sql, _args) = writer.finish();
        assert_eq!(
            sql,
            "SELECT * FROM [dbo].[wormx_users] WITH (ROWLOCK, READPAST) OPTION (RECOMPILE)"
        );

        let mut writer = SqlWriter::new(MssqlDialect);
        let mut projection = StarProjection;
        let mut filters = Vec::new();
        let mut joins = vec![wormx_core::Join::inner(
            TableSource::new(&TABLE, Some("u")).with_suffix("READPAST"),
            Expr::<bool, MssqlDialect>::raw("1 = 1"),
        )];
        let mut groups = Vec::new();
        let mut having = Vec::new();
        let mut orders = Vec::new();
        SqlSelect
            .write(
                wormx_core::SelectPlan {
                    table: TableSource::new(&TABLE, None),
                    distinct: false,
                    projection: &mut projection,
                    ctes: &mut [],
                    filters: &mut filters,
                    joins: &mut joins,
                    groups: &mut groups,
                    having: &mut having,
                    orders: &mut orders,
                    limit: None,
                    offset: None,
                    tail: None,
                },
                &mut writer,
            )
            .expect("render select with hinted join");
        let (sql, _args) = writer.finish();
        assert_eq!(
            sql,
            "SELECT * FROM [dbo].[wormx_users] JOIN [dbo].[wormx_users] AS [u] WITH (READPAST) ON 1 = 1"
        );

        let mut writer = SqlWriter::new(MssqlDialect);
        let mut filters = Vec::new();
        let mut joins = Vec::new();
        SqlSelect
            .write_count_star(
                wormx_core::CountPlan {
                    table: TableSource::new(&TABLE, None).hint(TableHint::RowLock),
                    distinct: false,
                    filters: &mut filters,
                    joins: &mut joins,
                    ctes: &mut [],
                    tail: Some("RECOMPILE"),
                },
                &mut writer,
            )
            .expect("render count");
        let (sql, _args) = writer.finish();
        assert_eq!(
            sql,
            "SELECT COUNT_BIG(*) FROM [dbo].[wormx_users] WITH (ROWLOCK) OPTION (RECOMPILE)"
        );

        let mut writer = SqlWriter::new(MssqlDialect);
        let mut assignments = vec![("name", Expr::<(), MssqlDialect>::raw("?"))];
        let mut filters = Vec::new();
        SqlUpdate
            .write(
                wormx_core::UpdatePlan {
                    table: TableSource::new(&TABLE, None).with_suffix("ROWLOCK"),
                    assignments: &mut assignments,
                    filters: &mut filters,
                    returning: false,
                },
                &mut writer,
            )
            .expect("render update");
        let (sql, _args) = writer.finish();
        assert_eq!(
            sql,
            "UPDATE [dbo].[wormx_users] WITH (ROWLOCK) SET [name] = ?"
        );

        let mut writer = SqlWriter::new(MssqlDialect);
        let mut filters = Vec::new();
        SqlDelete
            .write(
                wormx_core::DeletePlan {
                    table: TableSource::new(&TABLE, None).with_suffix("READPAST"),
                    filters: &mut filters,
                    returning: false,
                },
                &mut writer,
            )
            .expect("render delete");
        let (sql, _args) = writer.finish();
        assert_eq!(sql, "DELETE FROM [dbo].[wormx_users] WITH (READPAST)");
    }

    #[test]
    fn merge_combines_holdlock_with_caller_hints() {
        let columns = ["id"];
        let conflict_columns = ["id"];
        let mut values = vec![Expr::<(), MssqlDialect>::raw("?")];
        let mut assignments = vec![("id", Expr::<(), MssqlDialect>::raw("?"))];
        let plan = UpsertPlan {
            table: TableSource::new(&TABLE, None).with_suffix("ROWLOCK"),
            columns: &columns,
            values: &mut values,
            conflict_columns: &conflict_columns,
            assignments: &mut assignments,
            returning: false,
        };

        let mut writer = SqlWriter::new(MssqlDialect);
        MssqlUpsert.write(plan, &mut writer).expect("render merge");
        let (sql, _args) = writer.finish();
        assert!(
            sql.starts_with("MERGE INTO [dbo].[wormx_users] WITH (HOLDLOCK, ROWLOCK)"),
            "{sql}"
        );
    }
}
