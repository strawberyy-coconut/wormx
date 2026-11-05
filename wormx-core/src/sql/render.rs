//! SQL implementations of the select and data-modification renderers.
//!
//! The plan structs and renderer traits live in [`crate::plan::query`]. This
//! module is the SQL interpretation of them; another query language supplies
//! its own implementations of the same traits.

use sqlx_core::error::{Error, Result};

use crate::plan::join::{Join, TableSource, push_table};
use crate::plan::query::{
    CountPlan, CountProjection, DeletePlan, DeleteRenderer, InsertPlan, InsertRenderer,
    ProjectionWriter, SelectPlan, SelectRenderer, UpdatePlan, UpdateRenderer, UpsertPlan,
    UpsertRenderer,
};

use super::{Dialect, Expr, Order, SqlWriter};

/// The generic SQL renderer for [`SelectRenderer`].
#[derive(Debug, Clone, Copy, Default)]
pub struct SqlSelect;

impl<D: Dialect> SelectRenderer<D> for SqlSelect {
    fn write<P>(&self, plan: SelectPlan<'_, D, P>, writer: &mut SqlWriter<'_, D>) -> Result<()>
    where
        P: ProjectionWriter<D>,
    {
        writer.write_select(plan.distinct);
        plan.projection.write(writer)?;
        write_from_filters(writer, plan.table, plan.filters, plan.joins)?;
        write_groups_having(writer, plan.groups, plan.having)?;
        write_orders(writer, plan.orders)?;
        writer.push_limit_offset(!plan.orders.is_empty(), plan.limit, plan.offset);
        write_tail(writer, plan.tail);
        Ok(())
    }

    fn write_count<P>(
        &self,
        plan: SelectPlan<'_, D, P>,
        writer: &mut SqlWriter<'_, D>,
        distinct: bool,
    ) -> Result<()>
    where
        P: ProjectionWriter<D> + CountProjection<D>,
    {
        if distinct {
            writer.write_select(false);
            plan.projection.write_count(writer, true)?;
            write_from_filters(writer, plan.table, plan.filters, plan.joins)?;
            write_tail(writer, plan.tail);
            return Ok(());
        }

        // `SELECT COUNT(*)` over a DISTINCT or grouped query must count the
        // resulting rows, not the rows before DISTINCT/GROUP BY.
        if plan.distinct || !plan.groups.is_empty() || !plan.having.is_empty() {
            writer.write_select(false);
            writer.write_count_star();
            writer.push(" FROM (");
            writer.write_select(plan.distinct);
            plan.projection.write(writer)?;
            write_from_filters(writer, plan.table, plan.filters, plan.joins)?;
            write_groups_having(writer, plan.groups, plan.having)?;
            writer.push(") AS ");
            writer.push_ident("wormx_count");
            write_tail(writer, plan.tail);
            return Ok(());
        }

        writer.write_select(false);
        plan.projection.write_count(writer, false)?;
        write_from_filters(writer, plan.table, plan.filters, plan.joins)?;
        write_tail(writer, plan.tail);
        Ok(())
    }

    fn write_count_star(
        &self,
        plan: CountPlan<'_, D>,
        writer: &mut SqlWriter<'_, D>,
    ) -> Result<()> {
        if plan.distinct {
            writer.write_select(false);
            writer.write_count_star();
            writer.push(" FROM (");
            writer.write_select(true);
            writer.write_star();
            write_from_filters(writer, plan.table, plan.filters, plan.joins)?;
            writer.push(") AS ");
            writer.push_ident("wormx_count");
            write_tail(writer, plan.tail);
            return Ok(());
        }

        writer.write_select(false);
        writer.write_count_star();
        write_from_filters(writer, plan.table, plan.filters, plan.joins)?;
        write_tail(writer, plan.tail);
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SqlInsert;

impl<D: Dialect> InsertRenderer<D> for SqlInsert {
    fn write(&self, plan: InsertPlan<'_, D>, writer: &mut SqlWriter<'_, D>) -> Result<()> {
        writer.write_insert();
        push_table(writer, plan.table.schema, plan.table.name);

        if plan.columns.is_empty() {
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
            writer.write_values();

            for (index, value) in plan.values.iter_mut().enumerate() {
                if index > 0 {
                    writer.write_comma();
                }
                value.render(writer)?;
            }
            writer.write_group_end();
        }

        if plan.returning {
            writer.write_returning();
        }

        Ok(())
    }
}

/// The generic SQL `UPDATE` renderer.
#[derive(Debug, Clone, Copy, Default)]
pub struct SqlUpdate;

impl<D: Dialect> UpdateRenderer<D> for SqlUpdate {
    fn write(&self, plan: UpdatePlan<'_, D>, writer: &mut SqlWriter<'_, D>) -> Result<()> {
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

        write_filters(writer, plan.filters)?;

        if plan.returning {
            writer.write_returning();
        }

        Ok(())
    }
}

/// The generic SQL `DELETE` renderer.
#[derive(Debug, Clone, Copy, Default)]
pub struct SqlDelete;

impl<D: Dialect> DeleteRenderer<D> for SqlDelete {
    fn write(&self, plan: DeletePlan<'_, D>, writer: &mut SqlWriter<'_, D>) -> Result<()> {
        writer.write_delete();
        plan.table.render(writer);
        write_filters(writer, plan.filters)?;

        if plan.returning {
            writer.write_returning();
        }

        Ok(())
    }
}

/// The generic SQL `UPSERT` renderer.
///
/// Renders `INSERT ... ON CONFLICT ... DO UPDATE`.
#[derive(Debug, Clone, Copy, Default)]
pub struct SqlUpsert;

impl<D: Dialect> UpsertRenderer<D> for SqlUpsert {
    fn write(&self, plan: UpsertPlan<'_, D>, writer: &mut SqlWriter<'_, D>) -> Result<()> {
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
            writer.write_returning();
        }

        Ok(())
    }
}

/// Writes a `WHERE` clause for a list of filters.
pub fn write_filters<D: Dialect>(
    writer: &mut SqlWriter<'_, D>,
    filters: &mut [Expr<bool, D>],
) -> Result<()> {
    if filters.is_empty() {
        return Ok(());
    }

    writer.write_where();
    for (index, filter) in filters.iter_mut().enumerate() {
        if index > 0 {
            writer.write_and();
        }
        writer.write_group_start();
        filter.render(writer)?;
        writer.write_group_end();
    }

    Ok(())
}

/// Writes `FROM`, joins, and the `WHERE` clause.
///
/// This is public so an external [`SelectRenderer`] can override one clause
/// without copying the whole generic renderer.
pub fn write_from_filters<D: Dialect>(
    writer: &mut SqlWriter<'_, D>,
    table: TableSource,
    filters: &mut [Expr<bool, D>],
    joins: &mut [Join<D>],
) -> Result<()> {
    writer.write_from();
    table.render(writer);

    for join in joins {
        join.render(writer)?;
    }

    write_filters(writer, filters)
}

/// Writes `GROUP BY` and `HAVING`.
pub fn write_groups_having<D: Dialect>(
    writer: &mut SqlWriter<'_, D>,
    groups: &mut [Expr<(), D>],
    having: &mut [Expr<bool, D>],
) -> Result<()> {
    if !groups.is_empty() {
        writer.write_group_by();
        for (index, group) in groups.iter_mut().enumerate() {
            if index > 0 {
                writer.write_comma();
            }
            group.render(writer)?;
        }
    }

    if !having.is_empty() {
        writer.write_having();
        for (index, filter) in having.iter_mut().enumerate() {
            if index > 0 {
                writer.write_and();
            }
            writer.write_group_start();
            filter.render(writer)?;
            writer.write_group_end();
        }
    }

    Ok(())
}

/// Writes an `ORDER BY` clause.
pub fn write_orders<D: Dialect>(
    writer: &mut SqlWriter<'_, D>,
    orders: &mut [Order<D>],
) -> Result<()> {
    if !orders.is_empty() {
        writer.write_order_by();
        for (index, order) in orders.iter_mut().enumerate() {
            if index > 0 {
                writer.write_comma();
            }
            order.render(writer)?;
        }
    }

    Ok(())
}

/// Writes an opaque, dialect-defined statement tail, if any.
pub fn write_tail<D: Dialect>(writer: &mut SqlWriter<'_, D>, tail: Option<&str>) {
    if let Some(tail) = tail {
        writer.write_tail(tail);
    }
}
