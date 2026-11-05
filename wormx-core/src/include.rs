//! Eager-loading steps applied after a base query.

use std::future::Future;
use std::pin::Pin;

use sqlx_core::error::Result;

use crate::db::{Db, Transaction};
use crate::dialect::Dialect;

/// An eager-loading step applied after the base query has produced rows.
pub trait Include<D: Dialect + Clone, T>: Send + Sync {
    fn apply<'a>(
        &'a self,
        db: &'a Db<D>,
        rows: &'a mut [T],
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

    /// Applies the include inside an open transaction.
    fn apply_in<'a>(
        &'a self,
        db: &'a Db<D>,
        tx: &'a mut Transaction<'_, D>,
        rows: &'a mut [T],
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        let _ = tx;
        self.apply(db, rows)
    }
}
