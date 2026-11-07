use sqlx_core::error::{Error, Result};

use crate::plan::expr::Expr;
use crate::plan::query::{
    DeletePlan, DeleteRenderer, InsertManyPlan, InsertPlan, InsertRenderer, UpdatePlan,
    UpdateRenderer,
};
use crate::sql::{ReturningOp, ReturningPosition, SqlDialect, SqlWriter};

use super::helpers::write_filters;
use super::select::push_table;

#[derive(Debug, Clone, Copy, Default)]
pub struct SqlInsert;

/// Writes `VALUES` followed by one parenthesized row per entry.
///
/// The first opening parenthesis comes from the dialect's `write_values` hook.
pub fn write_values_rows<D: SqlDialect>(
    writer: &mut SqlWriter<D>,
    rows: &mut [Vec<Expr<(), D>>],
) -> Result<()> {
    if rows.is_empty() {
        return Err(Error::InvalidArgument(
            "batch operation requires at least one row".to_owned(),
        ));
    }

    writer.write_values();
    for (index, row) in rows.iter_mut().enumerate() {
        if index > 0 {
            writer.write_comma();
            writer.write_group_start();
        }
        for (value_index, value) in row.iter_mut().enumerate() {
            if value_index > 0 {
                writer.write_comma();
            }
            value.render(writer)?;
        }
        writer.write_group_end();
    }
    Ok(())
}

impl<D: SqlDialect> InsertRenderer<D> for SqlInsert {
    fn write(&self, plan: InsertPlan<'_, D>, writer: &mut SqlWriter<D>) -> Result<()> {
        writer.write_insert();
        push_table(writer, plan.table.schema, plan.table.name);

        let returning_position = plan
            .returning
            .then(|| writer.returning_position(ReturningOp::Insert));

        if plan.columns.is_empty() {
            if returning_position == Some(ReturningPosition::BeforeValues) {
                writer.write_returning_clause(ReturningOp::Insert);
            }
            writer.write_insert_default();
        } else {
            writer.write_group_start();
            for (index, column) in plan.columns.iter().enumerate() {
                if index > 0 {
                    writer.write_comma();
                }
                writer.push_ident(column);
            }
            writer.write_group_end();

            if returning_position == Some(ReturningPosition::BeforeValues) {
                writer.write_returning_clause(ReturningOp::Insert);
            }

            writer.write_values();

            for (index, value) in plan.values.iter_mut().enumerate() {
                if index > 0 {
                    writer.write_comma();
                }
                value.render(writer)?;
            }
            writer.write_group_end();
        }

        if plan.returning && returning_position != Some(ReturningPosition::BeforeValues) {
            writer.write_returning_clause(ReturningOp::Insert);
        }

        Ok(())
    }

    fn write_many(&self, plan: InsertManyPlan<'_, D>, writer: &mut SqlWriter<D>) -> Result<()> {
        if plan.rows.is_empty() {
            return Err(Error::InvalidArgument(
                "batch insert requires at least one row".to_owned(),
            ));
        }

        writer.write_insert();
        push_table(writer, plan.table.schema, plan.table.name);

        let returning_position = plan
            .returning
            .then(|| writer.returning_position(ReturningOp::Insert));

        if plan.columns.is_empty() {
            if plan.rows.len() > 1 {
                return Err(Error::InvalidArgument(
                    "a batch with more than one row requires at least one column".to_owned(),
                ));
            }
            if returning_position == Some(ReturningPosition::BeforeValues) {
                writer.write_returning_clause(ReturningOp::Insert);
            }
            writer.write_insert_default();
        } else {
            writer.write_group_start();
            for (index, column) in plan.columns.iter().enumerate() {
                if index > 0 {
                    writer.write_comma();
                }
                writer.push_ident(column);
            }
            writer.write_group_end();

            if returning_position == Some(ReturningPosition::BeforeValues) {
                writer.write_returning_clause(ReturningOp::Insert);
            }

            write_values_rows(writer, plan.rows)?;
        }

        if plan.returning && returning_position != Some(ReturningPosition::BeforeValues) {
            writer.write_returning_clause(ReturningOp::Insert);
        }

        Ok(())
    }
}

/// The generic SQL `UPDATE` renderer.
#[derive(Debug, Clone, Copy, Default)]
pub struct SqlUpdate;

impl<D: SqlDialect> UpdateRenderer<D> for SqlUpdate {
    fn write(&self, plan: UpdatePlan<'_, D>, writer: &mut SqlWriter<D>) -> Result<()> {
        if plan.assignments.is_empty() {
            return Err(Error::InvalidArgument(
                "UPDATE requires at least one assignment; call .set(...) or .set_raw(...)"
                    .to_owned(),
            ));
        }

        writer.write_update();
        plan.table.render(writer);
        writer.write_set();

        for (index, (column, value)) in plan.assignments.iter_mut().enumerate() {
            if index > 0 {
                writer.write_comma();
            }
            writer.push_ident(column);
            writer.write_equals();
            value.render(writer)?;
        }

        let returning_position = plan
            .returning
            .then(|| writer.returning_position(ReturningOp::Update));

        if returning_position == Some(ReturningPosition::BeforeWhere) {
            writer.write_returning_clause(ReturningOp::Update);
        }

        write_filters(writer, plan.filters)?;

        if plan.returning && returning_position != Some(ReturningPosition::BeforeWhere) {
            writer.write_returning_clause(ReturningOp::Update);
        }

        Ok(())
    }
}

/// The generic SQL `DELETE` renderer.
#[derive(Debug, Clone, Copy, Default)]
pub struct SqlDelete;

impl<D: SqlDialect> DeleteRenderer<D> for SqlDelete {
    fn write(&self, plan: DeletePlan<'_, D>, writer: &mut SqlWriter<D>) -> Result<()> {
        writer.write_delete();
        plan.table.render(writer);

        let returning_position = plan
            .returning
            .then(|| writer.returning_position(ReturningOp::Delete));

        if returning_position == Some(ReturningPosition::BeforeWhere) {
            writer.write_returning_clause(ReturningOp::Delete);
        }

        write_filters(writer, plan.filters)?;

        if plan.returning && returning_position != Some(ReturningPosition::BeforeWhere) {
            writer.write_returning_clause(ReturningOp::Delete);
        }

        Ok(())
    }
}
