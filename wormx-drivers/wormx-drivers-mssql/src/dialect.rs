use wormx_core::{
    Dialect, ReturningOp, ReturningPosition, SqlSyntax, SqlWriter, SupportsFullJoin,
    SupportsFullTextSearch, SupportsReturning, SupportsSourceSuffix, SupportsStatementTail,
    SupportsUpsert,
};

use super::upsert::MssqlUpsert;

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

    fn write_input_column(column: &str, writer: &mut SqlWriter<Self>) {
        writer.push_ident("source");
        writer.write_identifier_separator();
        writer.push_ident(column);
    }

    fn write_boolean_literal(value: bool, writer: &mut SqlWriter<Self>) {
        writer.push(if value { "1 = 1" } else { "1 = 0" });
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

impl SupportsFullJoin for MssqlDialect {}
impl SupportsSourceSuffix for MssqlDialect {}
impl SupportsStatementTail for MssqlDialect {}
impl SupportsFullTextSearch for MssqlDialect {}
impl SupportsReturning for MssqlDialect {}
impl SupportsUpsert for MssqlDialect {}
