//! Built-in driver implementations for wormx.
//!
//! Each driver has its own subcrate:
//!
//! - `wormx-drivers-postgres`
//! - `wormx-drivers-sqlite`
//! - `wormx-drivers-mysql`
//!
//! This crate re-exports them behind feature flags so applications can depend on one crate.

#[cfg(feature = "postgres")]
pub use wormx_drivers_postgres::*;

#[cfg(feature = "sqlite")]
pub use wormx_drivers_sqlite::*;

#[cfg(feature = "mysql")]
pub use wormx_drivers_mysql::*;

#[cfg(feature = "mssql")]
pub use wormx_drivers_mssql::*;
