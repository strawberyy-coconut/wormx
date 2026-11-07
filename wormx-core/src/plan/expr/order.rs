use crate::plan::backend::Dialect;

use super::{Expr, IntoExpr};

/// A rendered `ORDER BY` term.
pub struct Order<D: Dialect> {
    pub(super) node: Box<Expr<(), D>>,
    pub(super) descending: bool,
}

impl<D: Dialect> Order<D> {
    pub fn expr(&self) -> &Expr<(), D> {
        &self.node
    }

    pub fn expr_mut(&mut self) -> &mut Expr<(), D> {
        &mut self.node
    }

    pub fn descending(&self) -> bool {
        self.descending
    }
}

/// A window specification: `PARTITION BY ... ORDER BY ...`.
pub struct Window<D: Dialect> {
    pub(super) partition_by: Vec<Expr<(), D>>,
    pub(super) order_by: Vec<Order<D>>,
}

impl<D: Dialect> Window<D> {
    pub fn new() -> Self {
        Self {
            partition_by: Vec::new(),
            order_by: Vec::new(),
        }
    }

    pub fn partition_by<E, V>(mut self, expression: V) -> Self
    where
        V: IntoExpr<E, D>,
    {
        self.partition_by.push(expression.into_expr().erase());
        self
    }

    pub fn order_by(mut self, order: Order<D>) -> Self {
        self.order_by.push(order);
        self
    }

    pub fn partition_by_items(&self) -> &[Expr<(), D>] {
        &self.partition_by
    }

    pub fn partition_by_items_mut(&mut self) -> &mut [Expr<(), D>] {
        &mut self.partition_by
    }

    pub fn order_by_items(&self) -> &[Order<D>] {
        &self.order_by
    }

    pub fn order_by_items_mut(&mut self) -> &mut [Order<D>] {
        &mut self.order_by
    }
}

impl<D: Dialect> Default for Window<D> {
    fn default() -> Self {
        Self::new()
    }
}
