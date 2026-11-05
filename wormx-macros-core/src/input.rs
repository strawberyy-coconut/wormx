use proc_macro2::Span;
use syn::parse::{Parse, ParseStream};
use wormx_core::TableRef;

pub struct TableMacroInput {
    pub table: TableRef,
    pub span: Span,
}

impl Parse for TableMacroInput {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let literal: syn::LitStr = input.parse()?;
        let span = literal.span();
        let table = TableRef::parse(&literal.value())
            .map_err(|error| syn::Error::new(span, error.to_string()))?;

        if !input.is_empty() {
            return Err(input.error(
                "expected a single table name, e.g. `table!(\"users\")` or `table!(\"schema.users\")`",
            ));
        }

        Ok(Self { table, span })
    }
}
