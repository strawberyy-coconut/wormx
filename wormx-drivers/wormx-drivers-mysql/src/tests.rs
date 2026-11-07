use wormx_core::{
    Column, ColumnDef, Expr, InsertManyPlan, InsertRenderer, SqlInsert, SqlSyntax, SqlWriter,
    TableDef, TableSource, UpsertManyPlan, UpsertRenderer,
};

use super::{MySqlDialect, MySqlUpsert};

static COLUMNS: &[ColumnDef] = &[
    ColumnDef {
        name: "id",
        sql_type: "bigint",
        nullable: false,
        primary_key: true,
        has_default: false,
        identity: false,
        generated: false,
    },
    ColumnDef {
        name: "name",
        sql_type: "varchar",
        nullable: false,
        primary_key: false,
        has_default: false,
        identity: false,
        generated: false,
    },
];
static PRIMARY_KEY: &[&str] = &["id"];
static TABLE: TableDef = TableDef {
    name: "users",
    schema: None,
    columns: COLUMNS,
    primary_key: PRIMARY_KEY,
    relations: &[],
};

#[test]
fn empty_insert_uses_mysql_syntax() {
    let mut writer = SqlWriter::new(MySqlDialect);
    <MySqlDialect as SqlSyntax>::write_insert_default(&mut writer);
    let (sql, _) = writer.finish();
    assert_eq!(sql, " () VALUES ()");
}

#[test]
fn renders_multi_row_insert() {
    let columns = ["id", "name"];
    let mut rows = vec![
        vec![Expr::<(), MySqlDialect>::raw("?"), Expr::raw("?")],
        vec![Expr::raw("?"), Expr::raw("?")],
    ];

    let mut writer = SqlWriter::new(MySqlDialect);
    SqlInsert
        .write_many(
            InsertManyPlan {
                table: &TABLE,
                columns: &columns,
                rows: &mut rows,
                returning: false,
            },
            &mut writer,
        )
        .expect("render batch insert");
    let (sql, _) = writer.finish();

    assert_eq!(
        sql,
        "INSERT INTO `users`(`id`, `name`) VALUES (?, ?), (?, ?)"
    );
}

#[test]
fn renders_multi_row_upsert_with_input_column() {
    let columns = ["id", "name"];
    let conflict_columns = ["id"];
    let mut rows = vec![
        vec![Expr::<(), MySqlDialect>::raw("?"), Expr::raw("?")],
        vec![Expr::raw("?"), Expr::raw("?")],
    ];
    let mut assignments = vec![(
        "name",
        Expr::<(), MySqlDialect>::input(Column::new(None, "users", "name")),
    )];

    let plan = UpsertManyPlan {
        table: TableSource::new(&TABLE, None),
        columns: &columns,
        rows: &mut rows,
        conflict_columns: &conflict_columns,
        assignments: &mut assignments,
        returning: false,
    };

    let mut writer = SqlWriter::new(MySqlDialect);
    MySqlUpsert
        .write_many(plan, &mut writer)
        .expect("render batch upsert");
    let (sql, _) = writer.finish();

    assert_eq!(
        sql,
        "INSERT INTO `users`(`id`, `name`) VALUES (?, ?), (?, ?) ON DUPLICATE KEY UPDATE `name` = VALUES(`name`)"
    );
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
