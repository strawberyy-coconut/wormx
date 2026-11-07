use sqlx_core::error::{Error, Result};
use wormx_core::{ReturningOp, SqlWriter, UpsertManyPlan, UpsertPlan, UpsertRenderer};

use super::dialect::MssqlDialect;

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

        write_insert_branch(writer, plan.columns, plan.table.def);

        if plan.returning {
            writer.write_returning_clause(ReturningOp::Upsert);
        }
        writer.push(";");

        Ok(())
    }

    fn write_many(
        &self,
        plan: UpsertManyPlan<'_, MssqlDialect>,
        writer: &mut SqlWriter<MssqlDialect>,
    ) -> Result<()> {
        if plan.rows.is_empty() {
            return Err(Error::InvalidArgument(
                "batch upsert requires at least one row".to_owned(),
            ));
        }
        if plan.conflict_columns.is_empty() {
            return Err(Error::InvalidArgument(
                "UPSERT requires at least one conflict column; call .conflict_on(...)".to_owned(),
            ));
        }

        writer.push("MERGE INTO ");
        push_table(writer, plan.table.def);

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

        writer.push(" USING (VALUES ");
        for (row_index, row) in plan.rows.iter_mut().enumerate() {
            if row_index > 0 {
                writer.write_comma();
            }
            writer.write_group_start();
            for (value_index, value) in row.iter_mut().enumerate() {
                if value_index > 0 {
                    writer.write_comma();
                }
                value.render(writer)?;
            }
            writer.write_group_end();
        }
        writer.push(") AS ");
        writer.push_ident("source");
        writer.write_group_start();
        for (index, column) in plan.columns.iter().enumerate() {
            if index > 0 {
                writer.write_comma();
            }
            writer.push_ident(column);
        }
        writer.write_group_end();

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

        write_insert_branch(writer, plan.columns, plan.table.def);

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

fn insertable_columns(columns: &[&'static str], table: &wormx_core::TableDef) -> Vec<&'static str> {
    columns
        .iter()
        .copied()
        .filter(|column| {
            table
                .columns
                .iter()
                .find(|definition| definition.name == *column)
                .map(|definition| !definition.identity && !definition.generated)
                .unwrap_or(true)
        })
        .collect()
}

fn write_insert_branch(
    writer: &mut SqlWriter<MssqlDialect>,
    columns: &[&'static str],
    table: &wormx_core::TableDef,
) {
    let insert_columns = insertable_columns(columns, table);
    writer.push(" WHEN NOT MATCHED THEN INSERT ");
    if insert_columns.is_empty() {
        writer.push("DEFAULT VALUES");
        return;
    }

    writer.push("(");
    for (index, column) in insert_columns.iter().enumerate() {
        if index > 0 {
            writer.write_comma();
        }
        writer.push_ident(column);
    }
    writer.push(") VALUES (");
    for (index, column) in insert_columns.iter().enumerate() {
        if index > 0 {
            writer.write_comma();
        }
        push_qualified(writer, "source", column);
    }
    writer.push(")");
}
