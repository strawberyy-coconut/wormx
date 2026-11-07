use super::*;
use wormx_core::{
    Column, ColumnDef, DeletePlan, DeleteRenderer, Expr, InsertManyPlan, InsertPlan,
    InsertRenderer, SelectRenderer, SqlDelete, SqlInsert, SqlSelect, SqlUpdate, SqlWriter,
    StarProjection, TableDef, TableSource, UpdatePlan, UpdateRenderer, UpsertManyPlan, UpsertPlan,
    UpsertRenderer,
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
        sql.ends_with("WHEN NOT MATCHED THEN INSERT ([name]) VALUES ([source].[name]);"),
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

#[test]
fn renders_multi_row_insert_with_output() {
    let columns = ["id", "name"];
    let mut rows = vec![
        vec![Expr::<(), MssqlDialect>::raw("?"), Expr::raw("?")],
        vec![Expr::raw("?"), Expr::raw("?")],
    ];

    let mut writer = SqlWriter::new(MssqlDialect);
    SqlInsert
        .write_many(
            InsertManyPlan {
                table: &TABLE,
                columns: &columns,
                rows: &mut rows,
                returning: true,
            },
            &mut writer,
        )
        .expect("render batch insert");
    let (sql, _args) = writer.finish();

    assert_eq!(
        sql,
        "INSERT INTO [dbo].[wormx_users]([id], [name]) OUTPUT INSERTED.* VALUES (?, ?), (?, ?)"
    );
}

#[test]
fn renders_multi_row_merge_with_input_column() {
    let columns = ["id", "name"];
    let conflict_columns = ["id"];
    let mut rows = vec![
        vec![Expr::<(), MssqlDialect>::raw("?"), Expr::raw("?")],
        vec![Expr::raw("?"), Expr::raw("?")],
    ];
    let mut assignments = vec![(
        "name",
        Expr::<(), MssqlDialect>::input(Column::new(Some("dbo"), "wormx_users", "name")),
    )];

    let plan = UpsertManyPlan {
        table: TableSource::new(&TABLE, None),
        columns: &columns,
        rows: &mut rows,
        conflict_columns: &conflict_columns,
        assignments: &mut assignments,
        returning: true,
    };

    let mut writer = SqlWriter::new(MssqlDialect);
    MssqlUpsert
        .write_many(plan, &mut writer)
        .expect("render batch merge");
    let (sql, _args) = writer.finish();

    assert!(
        sql.starts_with(
            "MERGE INTO [dbo].[wormx_users] WITH (HOLDLOCK) AS [target] USING (VALUES (?, ?), (?, ?)) AS [source]([id], [name]) ON [target].[id] = [source].[id]"
        ),
        "{sql}"
    );
    assert!(
        sql.contains("WHEN MATCHED THEN UPDATE SET [name] = [source].[name]"),
        "{sql}"
    );
    assert!(
        sql.contains("WHEN NOT MATCHED THEN INSERT ([name]) VALUES ([source].[name])"),
        "{sql}"
    );
    assert!(sql.ends_with("OUTPUT INSERTED.*;"), "{sql}");
}

#[test]
fn renders_sql_server_boolean_literals() {
    let mut writer = SqlWriter::new(MssqlDialect);
    let mut expr = Expr::<bool, MssqlDialect>::literal(false);
    expr.render(&mut writer).expect("render boolean literal");
    let (sql, _args) = writer.finish();
    assert_eq!(sql, "1 = 0");
}
