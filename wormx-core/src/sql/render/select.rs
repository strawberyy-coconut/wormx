use sqlx_core::error::{Error, Result};

use crate::plan::expr::{ColumnRef, ExprKind};
use crate::plan::join::{Join, JoinConstraint, JoinSource, TableSource};
use crate::plan::query::{
    CountPlan, CountProjection, Projection, ProjectionVisitor, SelectPlan, SelectRenderer,
};
use crate::sql::expr::{render_column_ref, render_expr_kind};
use crate::sql::{SqlDialect, SqlWriter, Writer};

use super::cte::write_ctes;
use super::helpers::{write_from_filters, write_groups_having, write_orders, write_tail};

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
        match (&self.kind, &self.constraint) {
            (crate::plan::join::JoinKind::Cross, JoinConstraint::None) => {}
            (crate::plan::join::JoinKind::Cross, _) => {
                return Err(Error::InvalidArgument(
                    "CROSS JOIN cannot have an ON, USING, or NATURAL constraint".to_owned(),
                ));
            }
            (_, JoinConstraint::None) => {
                return Err(Error::InvalidArgument(
                    "a non-CROSS join requires ON, USING, or NATURAL".to_owned(),
                ));
            }
            (_, JoinConstraint::Using([])) => {
                return Err(Error::InvalidArgument(
                    "USING requires at least one column".to_owned(),
                ));
            }
            _ => {}
        }

        match &self.constraint {
            JoinConstraint::Natural => writer.write_natural_join(self.kind),
            _ => writer.write_join(self.kind),
        }

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

        match &mut self.constraint {
            JoinConstraint::On(on) => {
                writer.write_on();
                on.render(writer)
            }
            JoinConstraint::Using(columns) => {
                writer.write_using();
                for (index, column) in columns.iter().enumerate() {
                    if index > 0 {
                        writer.write_comma();
                    }
                    writer.push_ident(column);
                }
                writer.write_group_end();
                Ok(())
            }
            JoinConstraint::Natural | JoinConstraint::None => Ok(()),
        }
    }
}

pub(super) fn push_table<D: SqlDialect>(writer: &mut Writer<D>, schema: Option<&str>, table: &str) {
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
