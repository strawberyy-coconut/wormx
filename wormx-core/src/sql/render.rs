use sqlx_core::error::{Error, Result};

use crate::plan::expr::{ColumnRef, Expr, ExprKind, Order};
use crate::plan::join::{Join, JoinSource, TableSource};
use crate::plan::query::{
    CountPlan, CountProjection, Cte, DeletePlan, DeleteRenderer, InsertPlan, InsertRenderer,
    Projection, ProjectionVisitor, SelectPlan, SelectRenderer, UpdatePlan, UpdateRenderer,
    UpsertPlan, UpsertRenderer,
};

use super::expr::{render_column_ref, render_expr_kind, render_order};
use super::{SqlDialect, SqlWriter, Writer};

impl TableSource {
    /// Writes the table, alias, and any dialect-defined suffix.
    pub fn render<D: SqlDialect>(&self, writer: &mut Writer<D>) {
        push_table(writer, self.def.schema, self.def.name);
        if let Some(alias) = self.alias {
            writer.write_as();
            writer.push_ident(alias);
        }
        if let Some(suffix) = self.suffix {
            writer.write_source_suffix(suffix);
        }
    }
}

impl<D: SqlDialect> Join<D> {
    /// Writes the join clause.
    pub fn render(&mut self, writer: &mut Writer<D>) -> Result<()> {
        writer.write_join(self.kind);
        match &mut self.source {
            JoinSource::Table(source) => source.render(writer),
            JoinSource::Derived { query, alias } => {
                writer.write_group_start();
                query.render(writer)?;
                writer.write_group_end();
                writer.write_as();
                writer.push_ident(alias);
            }
            JoinSource::Named(name) => writer.push_ident(name),
        }
        writer.write_on();
        self.on.render(writer)
    }
}

fn push_table<D: SqlDialect>(writer: &mut Writer<D>, schema: Option<&str>, table: &str) {
    if let Some(schema) = schema {
        writer.push_ident(schema);
        writer.write_identifier_separator();
    }
    writer.push_ident(table);
}

/// Renders a projection for a custom SQL renderer.
pub fn write_projection<D: SqlDialect, P: Projection<D>>(
    writer: &mut Writer<D>,
    projection: &mut P,
) -> Result<()> {
    projection.visit(&mut SqlProjectionVisitor::new(writer))
}

/// The generic SQL renderer for [`SelectRenderer`].
#[derive(Debug, Clone, Copy, Default)]
pub struct SqlSelect;

struct SqlProjectionVisitor<'a, D: SqlDialect> {
    writer: &'a mut Writer<D>,
}

impl<'a, D: SqlDialect> SqlProjectionVisitor<'a, D> {
    fn new(writer: &'a mut Writer<D>) -> Self {
        Self { writer }
    }
}

impl<D: SqlDialect> ProjectionVisitor<D> for SqlProjectionVisitor<'_, D> {
    fn star(&mut self) -> Result<()> {
        self.writer.write_star();
        Ok(())
    }

    fn column(&mut self, column: &ColumnRef) -> Result<()> {
        render_column_ref(column, self.writer);
        Ok(())
    }

    fn expr(&mut self, expr: &mut ExprKind<D>) -> Result<()> {
        render_expr_kind(expr, self.writer)
    }

    fn separator(&mut self) -> Result<()> {
        self.writer.write_comma();
        Ok(())
    }

    fn begin_count(&mut self, distinct: bool) -> Result<()> {
        self.writer.write_count_start(distinct);
        self.writer.write_group_start();
        if distinct {
            self.writer.push("DISTINCT ");
        }
        Ok(())
    }

    fn end_count(&mut self) -> Result<()> {
        self.writer.write_group_end();
        Ok(())
    }
}

impl<D: SqlDialect> SelectRenderer<D> for SqlSelect {
    fn write<P>(&self, plan: SelectPlan<'_, D, P>, writer: &mut SqlWriter<D>) -> Result<()>
    where
        P: Projection<D>,
    {
        write_ctes(writer, plan.ctes)?;
        writer.write_select(plan.distinct);
        plan.projection
            .visit(&mut SqlProjectionVisitor::new(writer))?;
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
        writer: &mut SqlWriter<D>,
        distinct: bool,
    ) -> Result<()>
    where
        P: Projection<D> + CountProjection<D>,
    {
        write_ctes(writer, plan.ctes)?;

        // A DISTINCT or grouped query must count the resulting rows, not the
        // rows before DISTINCT/GROUP BY.
        if plan.distinct || !plan.groups.is_empty() || !plan.having.is_empty() {
            writer.write_select(false);
            writer.write_count_star();
            writer.push(" FROM (");
            writer.write_select(plan.distinct);
            plan.projection
                .visit(&mut SqlProjectionVisitor::new(writer))?;
            write_from_filters(writer, plan.table, plan.filters, plan.joins)?;
            write_groups_having(writer, plan.groups, plan.having)?;
            writer.push(") AS ");
            writer.push_ident("wormx_count");
            write_tail(writer, plan.tail);
            return Ok(());
        }

        writer.write_select(false);
        {
            let mut visitor = SqlProjectionVisitor::new(writer);
            visitor.begin_count(distinct)?;
            plan.projection.visit(&mut visitor)?;
            visitor.end_count()?;
        }
        write_from_filters(writer, plan.table, plan.filters, plan.joins)?;
        write_tail(writer, plan.tail);
        Ok(())
    }

    fn write_count_star(&self, plan: CountPlan<'_, D>, writer: &mut SqlWriter<D>) -> Result<()> {
        write_ctes(writer, plan.ctes)?;

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

/// Writes a `WITH` clause.
pub fn write_ctes<D: SqlDialect>(writer: &mut Writer<D>, ctes: &mut [Cte<D>]) -> Result<()> {
    if ctes.is_empty() {
        return Ok(());
    }

    writer.push("WITH ");
    for (index, cte) in ctes.iter_mut().enumerate() {
        if index > 0 {
            writer.write_comma();
        }
        cte.render(writer)?;
    }
    writer.write_space();
    Ok(())
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SqlInsert;

impl<D: SqlDialect> InsertRenderer<D> for SqlInsert {
    fn write(&self, plan: InsertPlan<'_, D>, writer: &mut SqlWriter<D>) -> Result<()> {
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

impl<D: SqlDialect> DeleteRenderer<D> for SqlDelete {
    fn write(&self, plan: DeletePlan<'_, D>, writer: &mut SqlWriter<D>) -> Result<()> {
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
            writer.write_returning();
        }

        Ok(())
    }
}

/// Writes a `WHERE` clause for a list of filters.
pub fn write_filters<D: SqlDialect>(
    writer: &mut SqlWriter<D>,
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
pub fn write_from_filters<D: SqlDialect>(
    writer: &mut SqlWriter<D>,
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
pub fn write_groups_having<D: SqlDialect>(
    writer: &mut SqlWriter<D>,
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
pub fn write_orders<D: SqlDialect>(
    writer: &mut SqlWriter<D>,
    orders: &mut [Order<D>],
) -> Result<()> {
    if !orders.is_empty() {
        writer.write_order_by();
        for (index, order) in orders.iter_mut().enumerate() {
            if index > 0 {
                writer.write_comma();
            }
            render_order(order, writer)?;
        }
    }

    Ok(())
}

/// Writes an opaque, dialect-defined statement tail, if any.
pub fn write_tail<D: SqlDialect>(writer: &mut SqlWriter<D>, tail: Option<&str>) {
    if let Some(tail) = tail {
        writer.write_tail(tail);
    }
}
