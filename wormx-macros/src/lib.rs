//! The `wormx::table!` procedural macro.

use proc_macro::TokenStream;
use syn::parse_macro_input;
use wormx_macros_core::expand_table;

mod drivers;

/// Generates a Rust row type and table metadata for a database table.
///
/// ```ignore
/// wormx::table!("users");
/// wormx::table!("public.users");
/// ```
#[proc_macro]
pub fn table(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as wormx_macros_core::TableMacroInput);

    match expand_table(input, drivers::all()) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}
