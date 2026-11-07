use wormx_core::{
    Dialect, SqlSyntax, SqlWriter, SupportsFullTextSearch, SupportsJson, SupportsNaturalJoin,
    SupportsUpsert, SupportsUsing,
};

use super::upsert::MySqlUpsert;

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

    fn write_input_column(column: &str, writer: &mut SqlWriter<Self>) {
        writer.push("VALUES");
        writer.write_group_start();
        writer.push_ident(column);
        writer.write_group_end();
    }
}

impl SupportsUsing for MySqlDialect {}
impl SupportsNaturalJoin for MySqlDialect {}
impl SupportsFullTextSearch for MySqlDialect {}
impl SupportsJson for MySqlDialect {}
impl SupportsUpsert for MySqlDialect {}
