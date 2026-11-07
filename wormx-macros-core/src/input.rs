use proc_macro2::{Span, TokenStream};
use wormx_core::{RelationMode, TableRef};

mod parse;
#[cfg(test)]
mod tests;

pub struct TableMacroInput {
    pub table: TableRef,
    pub span: Span,
    /// Explicit Rust struct name from `table!("users" as User { ... })`.
    pub struct_name: Option<syn::Ident>,
    /// Table-level derives. When present, these replace `[wormx.codegen]` derives.
    pub derives: Option<Vec<syn::Path>>,
    /// Table-level attributes, written without the surrounding `#[...]`.
    pub attrs: Vec<TokenStream>,
    /// Attributes applied to every generated relation field.
    pub relation_attrs: Vec<TokenStream>,
    /// Relation generation mode from `relations(...)`.
    pub relations: Option<RelationMode>,
    /// Relation names to generate in `explicit` mode.
    pub include_relations: Vec<String>,
    /// Relation names to omit.
    pub skip_relations: Vec<String>,
    pub columns: Vec<ColumnMacroOptions>,
}

pub struct ColumnMacroOptions {
    /// The database column name.
    pub name: String,
    /// Optional Rust field name from `"column" as field`.
    pub field_name: Option<syn::Ident>,
    /// Field attributes, written without the surrounding `#[...]`.
    pub attrs: Vec<TokenStream>,
    /// Whether this column is omitted from the generated row type.
    pub skip: bool,
    pub span: Span,
}
