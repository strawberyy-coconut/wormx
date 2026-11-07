use sqlx_core::error::Result;

use crate::plan::expr::{Expr, Order};
use crate::plan::join::{Join, TableSource};
use crate::sql::expr::render_order;
use crate::sql::{SqlDialect, SqlWriter};

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
/// This is public so an external [`crate::plan::query::SelectRenderer`] can override one clause
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
