//! SQL renderers for query plans.
//!
//! - `select`: `SELECT`, joins, and projections.
//! - `cte`: common table expressions.
//! - `dml`: `INSERT`, `UPDATE`, and `DELETE`.
//! - `upsert`: `INSERT ... ON CONFLICT` / `MERGE`-style rendering.
//! - `helpers`: shared clause writers.

mod cte;
mod dml;
mod helpers;
mod select;
mod upsert;

pub use cte::write_ctes;
pub use dml::{SqlDelete, SqlInsert, SqlUpdate, write_values_rows};
pub use helpers::{
    write_filters, write_from_filters, write_groups_having, write_orders, write_tail,
};
pub use select::{SqlSelect, write_projection};
pub use upsert::SqlUpsert;
