use sqlx_core::error::{Error, Result};

use crate::plan::query::{UpsertManyPlan, UpsertPlan, UpsertRenderer};
use crate::sql::{ReturningOp, SqlDialect, SqlWriter};

use super::dml::write_values_rows;

#[derive(Debug, Clone, Copy, Default)]
pub struct SqlUpsert;

impl<D: SqlDialect> UpsertRenderer<D> for SqlUpsert {
    fn write(&self, plan: UpsertPlan<'_, D>, writer: &mut SqlWriter<D>) -> Result<()> {
        if plan.conflict_columns.is_empty() {
            return Err(Error::InvalidArgument(
                "UPSERT requires at least one conflict column; call .conflict_on(...)".to_owned(),
            ));
        }
        if plan.assignments.is_empty() {
            return Err(Error::InvalidArgument(
                "UPSERT requires at least one .update(...) assignment".to_owned(),
            ));
        }

        writer.write_insert();
        plan.table.render(writer);
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

        writer.write_on_conflict();
        writer.write_group_start();
        for (index, column) in plan.conflict_columns.iter().enumerate() {
            if index > 0 {
                writer.write_comma();
            }
            writer.push_ident(column);
        }
        writer.write_group_end();
        writer.write_do_update_set();

        for (index, (column, value)) in plan.assignments.iter_mut().enumerate() {
            if index > 0 {
                writer.write_comma();
            }
            writer.push_ident(column);
            writer.write_equals();
            value.render(writer)?;
        }

        if plan.returning {
            writer.write_returning_clause(ReturningOp::Upsert);
        }

        Ok(())
    }

    fn write_many(&self, plan: UpsertManyPlan<'_, D>, writer: &mut SqlWriter<D>) -> Result<()> {
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
        if plan.assignments.is_empty() {
            return Err(Error::InvalidArgument(
                "UPSERT requires at least one .update(...) or .update_from_input(...) assignment"
                    .to_owned(),
            ));
        }

        writer.write_insert();
        plan.table.render(writer);
        writer.write_group_start();
        for (index, column) in plan.columns.iter().enumerate() {
            if index > 0 {
                writer.write_comma();
            }
            writer.push_ident(column);
        }
        writer.write_group_end();

        write_values_rows(writer, plan.rows)?;

        writer.write_on_conflict();
        writer.write_group_start();
        for (index, column) in plan.conflict_columns.iter().enumerate() {
            if index > 0 {
                writer.write_comma();
            }
            writer.push_ident(column);
        }
        writer.write_group_end();
        writer.write_do_update_set();

        for (index, (column, value)) in plan.assignments.iter_mut().enumerate() {
            if index > 0 {
                writer.write_comma();
            }
            writer.push_ident(column);
            writer.write_equals();
            value.render(writer)?;
        }

        if plan.returning {
            writer.write_returning_clause(ReturningOp::Upsert);
        }

        Ok(())
    }
}
