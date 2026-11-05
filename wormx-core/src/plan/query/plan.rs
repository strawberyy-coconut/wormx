//! Plan structs and renderer traits.
//!
//! These describe what a renderer receives. The SQL implementations live in
//! [`crate::sql::render`]; a renderer for another query language can implement
//! the same traits.

use sqlx_core::error::Result;

use crate::plan::expr::{Expr, Order};
use crate::plan::join::{Join, TableSource};
use crate::plan::{Dialect, Writer};
use crate::schema::TableDef;

use super::{CountProjection, Cte, Projection};

/// All inputs needed to render a `SELECT` operation.
pub struct SelectPlan<'a, D: Dialect, P> {
    /// The base table, including an optional alias or hints.
    pub table: TableSource,
    pub distinct: bool,
    pub projection: &'a mut P,
    pub ctes: &'a mut [Cte<D>],
    pub filters: &'a mut [Expr<bool, D>],
    pub joins: &'a mut [Join<D>],
    pub groups: &'a mut [Expr<(), D>],
    pub having: &'a mut [Expr<bool, D>],
    pub orders: &'a mut [Order<D>],
    pub limit: Option<u64>,
    pub offset: Option<u64>,
    /// An opaque, dialect-defined statement tail.
    pub tail: Option<&'static str>,
}

/// Inputs needed to render `SELECT COUNT(*)`.
pub struct CountPlan<'a, D: Dialect> {
    /// The base table, including an optional alias or hints.
    pub table: TableSource,
    pub distinct: bool,
    pub filters: &'a mut [Expr<bool, D>],
    pub joins: &'a mut [Join<D>],
    pub ctes: &'a mut [Cte<D>],
    /// An opaque, dialect-defined statement tail.
    pub tail: Option<&'static str>,
}

/// Renders the `SELECT` operation for a dialect.
///
/// A dialect chooses its renderer with [`Dialect::Select`]. An external
/// renderer can reproduce the generic shape with the public helpers
/// [`crate::sql::render::write_from_filters`],
/// [`crate::sql::render::write_groups_having`],
/// [`crate::sql::render::write_orders`], and
/// [`crate::sql::render::write_tail`], plus [`Join::render`] and
/// [`TableSource::render`]; it does not need to copy the whole `SqlSelect`
/// body to support joins.
pub trait SelectRenderer<D: Dialect>: Default {
    /// Renders a row-producing select.
    fn write<P>(&self, plan: SelectPlan<'_, D, P>, writer: &mut Writer<D>) -> Result<()>
    where
        P: Projection<D>;

    /// Renders `SELECT COUNT(projection)` (or `COUNT(DISTINCT projection)`).
    fn write_count<P>(
        &self,
        plan: SelectPlan<'_, D, P>,
        writer: &mut Writer<D>,
        distinct: bool,
    ) -> Result<()>
    where
        P: Projection<D> + CountProjection<D>;

    /// Renders `SELECT COUNT(*)`.
    fn write_count_star(&self, plan: CountPlan<'_, D>, writer: &mut Writer<D>) -> Result<()>;
}

/// All inputs needed to render an `INSERT` operation.
pub struct InsertPlan<'a, D: Dialect> {
    pub table: &'static TableDef,
    pub columns: &'a [&'static str],
    pub values: &'a mut [Expr<(), D>],
    pub returning: bool,
}

/// All inputs needed to render an `UPDATE` operation.
pub struct UpdatePlan<'a, D: Dialect> {
    /// The target table, including optional hints.
    pub table: TableSource,
    pub assignments: &'a mut [(&'static str, Expr<(), D>)],
    pub filters: &'a mut [Expr<bool, D>],
    pub returning: bool,
}

/// All inputs needed to render a `DELETE` operation.
pub struct DeletePlan<'a, D: Dialect> {
    /// The target table, including optional hints.
    pub table: TableSource,
    pub filters: &'a mut [Expr<bool, D>],
    pub returning: bool,
}

/// All inputs needed to render an `UPSERT`/`MERGE` operation.
pub struct UpsertPlan<'a, D: Dialect> {
    /// The target table, including optional hints.
    pub table: TableSource,
    pub columns: &'a [&'static str],
    pub values: &'a mut [Expr<(), D>],
    pub conflict_columns: &'a [&'static str],
    pub assignments: &'a mut [(&'static str, Expr<(), D>)],
    pub returning: bool,
}

/// Renders an `INSERT` operation.
///
/// A dialect chooses this renderer with [`Dialect::Insert`].
pub trait InsertRenderer<D: Dialect>: Default {
    fn write(&self, plan: InsertPlan<'_, D>, writer: &mut Writer<D>) -> Result<()>;
}

/// Renders an `UPDATE` operation.
///
/// A dialect chooses this renderer with [`Dialect::Update`].
pub trait UpdateRenderer<D: Dialect>: Default {
    fn write(&self, plan: UpdatePlan<'_, D>, writer: &mut Writer<D>) -> Result<()>;
}

/// Renders a `DELETE` operation.
///
/// A dialect chooses this renderer with [`Dialect::Delete`].
pub trait DeleteRenderer<D: Dialect>: Default {
    fn write(&self, plan: DeletePlan<'_, D>, writer: &mut Writer<D>) -> Result<()>;
}

/// Renders an `UPSERT`/`MERGE` operation.
///
/// A dialect chooses this renderer with [`Dialect::Upsert`].
pub trait UpsertRenderer<D: Dialect>: Default {
    fn write(&self, plan: UpsertPlan<'_, D>, writer: &mut Writer<D>) -> Result<()>;
}
