//! Registry of the built-in compile-time drivers.
//!
//! This is the only place that knows the list of bundled backends. A
//! third-party backend does not need to edit it: depend on `wormx-macros-core`
//! and call [`wormx_macros_core::expand_table`] with your own
//! [`wormx_macros_core::TableDriver`], or expose a `table!` macro from your
//! own proc-macro crate.

use wormx_macros_core::TableDriver;

const BUILTIN: &[TableDriver] = &[
    #[cfg(feature = "postgres")]
    wormx_drivers::POSTGRES,
    #[cfg(feature = "sqlite")]
    wormx_drivers::SQLITE,
    #[cfg(feature = "mysql")]
    wormx_drivers::MYSQL,
    #[cfg(feature = "mssql")]
    wormx_drivers::MSSQL,
];

/// Returns the drivers enabled by this crate's features.
pub(crate) fn all() -> &'static [TableDriver] {
    BUILTIN
}
