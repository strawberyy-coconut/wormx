//! MySQL support for wormx, built on `sqlx-mysql`.

mod dialect;
mod upsert;

#[cfg(feature = "macros")]
mod macros;
#[cfg(test)]
mod tests;

pub use dialect::MySqlDialect;
pub use upsert::MySqlUpsert;

#[cfg(feature = "macros")]
pub use macros::MYSQL;
