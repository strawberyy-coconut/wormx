//! Microsoft SQL Server support for wormx, built on `sqlx-mssql-rs`.

mod dialect;
mod ext;
mod upsert;

#[cfg(feature = "macros")]
mod macros;
#[cfg(test)]
mod tests;

pub use dialect::MssqlDialect;
pub use ext::{IntoTableHint, MssqlStatementExt, MssqlTableExt, TableHint};
pub use upsert::MssqlUpsert;

#[cfg(feature = "macros")]
pub use macros::MSSQL;
