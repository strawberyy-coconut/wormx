use std::marker::PhantomData;

use sqlx_core::error::Result;

use super::cte::Cte;
use super::plan::{CountPlan, SelectPlan, SelectRenderer};
use super::projection::{CountProjection, Projection};
use crate::exec::Db;
use crate::plan::backend::Dialect;
use crate::plan::command::Writer;
use crate::plan::expr::{Expr, Order};
use crate::plan::join::{Join, TableSource};
use crate::schema::Table;

/// Shared builder state for row and projected selects.
pub(super) struct SelectState<D: Dialect + Clone, T: 'static> {
    pub(super) db: Db<D>,
    pub(super) source: TableSource,
    pub(super) distinct: bool,
    pub(super) filters: Vec<Expr<bool, D>>,
    pub(super) joins: Vec<Join<D>>,
    pub(super) orders: Vec<Order<D>>,
    pub(super) ctes: Vec<Cte<D>>,
    pub(super) limit: Option<u64>,
    pub(super) offset: Option<u64>,
    pub(super) tail: Option<&'static str>,
    pub(super) _marker: PhantomData<fn() -> T>,
}

impl<D: Dialect + Clone, T: Table + 'static> SelectState<D, T> {
    pub(super) fn new(db: Db<D>) -> Self {
        Self {
            db,
            source: TableSource::new(T::DEF, None),
            distinct: false,
            filters: Vec::new(),
            joins: Vec::new(),
            orders: Vec::new(),
            ctes: Vec::new(),
            limit: None,
            offset: None,
            tail: None,
            _marker: PhantomData,
        }
    }

    pub(super) fn filter(mut self, filter: Expr<bool, D>) -> Self {
        self.filters.push(filter);
        self
    }

    pub(super) fn distinct(mut self) -> Self {
        self.distinct = true;
        self
    }

    pub(super) fn with(mut self, cte: Cte<D>) -> Self {
        self.ctes.push(cte);
        self
    }

    pub(super) fn join(mut self, join: Join<D>) -> Self {
        self.joins.push(join);
        self
    }

    pub(super) fn order_by(mut self, order: Order<D>) -> Self {
        self.orders.push(order);
        self
    }

    pub(super) fn limit(mut self, limit: u64) -> Self {
        self.limit = Some(limit);
        self
    }

    pub(super) fn offset(mut self, offset: u64) -> Self {
        self.offset = Some(offset);
        self
    }

    pub(super) fn with_source_suffix(mut self, suffix: &'static str) -> Self {
        self.source.suffix = Some(suffix);
        self
    }

    pub(super) fn with_tail(mut self, tail: &'static str) -> Self {
        self.tail = Some(tail);
        self
    }

    pub(super) fn write<P: Projection<D>>(
        &mut self,
        projection: &mut P,
        groups: &mut [Expr<(), D>],
        having: &mut [Expr<bool, D>],
        writer: &mut Writer<D>,
    ) -> Result<()> {
        D::Select::default().write(
            SelectPlan {
                table: self.source,
                distinct: self.distinct,
                projection,
                ctes: &mut self.ctes,
                filters: &mut self.filters,
                joins: &mut self.joins,
                groups,
                having,
                orders: &mut self.orders,
                limit: self.limit,
                offset: self.offset,
                tail: self.tail,
            },
            writer,
        )
    }

    pub(super) fn write_count<P>(
        &mut self,
        projection: &mut P,
        groups: &mut [Expr<(), D>],
        having: &mut [Expr<bool, D>],
        writer: &mut Writer<D>,
        distinct: bool,
    ) -> Result<()>
    where
        P: Projection<D> + CountProjection<D>,
    {
        D::Select::default().write_count(
            SelectPlan {
                table: self.source,
                distinct: self.distinct,
                projection,
                ctes: &mut self.ctes,
                filters: &mut self.filters,
                joins: &mut self.joins,
                groups,
                having,
                orders: &mut self.orders,
                limit: self.limit,
                offset: self.offset,
                tail: self.tail,
            },
            writer,
            distinct,
        )
    }

    pub(super) fn write_count_star(&mut self, writer: &mut Writer<D>) -> Result<()> {
        D::Select::default().write_count_star(
            CountPlan {
                table: self.source,
                distinct: self.distinct,
                filters: &mut self.filters,
                joins: &mut self.joins,
                ctes: &mut self.ctes,
                tail: self.tail,
            },
            writer,
        )
    }
}
