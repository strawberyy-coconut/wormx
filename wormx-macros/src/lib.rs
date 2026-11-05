//! The `wormx::table!` procedural macro.

use proc_macro::TokenStream;
use syn::parse_macro_input;
use wormx_macros_core::{TableDriver, expand_table};

const DRIVERS: &[TableDriver] = &[
    #[cfg(feature = "postgres")]
    wormx_drivers::POSTGRES,
    #[cfg(feature = "sqlite")]
    wormx_drivers::SQLITE,
    #[cfg(feature = "mysql")]
    wormx_drivers::MYSQL,
    #[cfg(feature = "mssql")]
    wormx_drivers::MSSQL,
];

/// Generates a Rust row type and table metadata for a database table.
///
/// ```ignore
/// wormx::table!("users");
/// wormx::table!("public.users");
/// ```
#[proc_macro]
pub fn table(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as wormx_macros_core::TableMacroInput);

    match expand_table(input, DRIVERS) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}
