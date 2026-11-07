use sqlx_core::error::{Error, Result};
use wormx_core::{SqlWriter, UpsertManyPlan, UpsertPlan, UpsertRenderer, write_values_rows};

use super::dialect::MySqlDialect;

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

    fn write_many(
        &self,
        plan: UpsertManyPlan<'_, MySqlDialect>,
        writer: &mut SqlWriter<MySqlDialect>,
    ) -> Result<()> {
        if plan.assignments.is_empty() {
            return Err(Error::InvalidArgument(
                "UPSERT requires at least one .update(...) or .update_from_input(...) assignment for MySQL"
                    .to_owned(),
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

        write_values_rows(writer, plan.rows)?;

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
