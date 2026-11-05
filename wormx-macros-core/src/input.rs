use std::collections::HashSet;

use proc_macro2::{Span, TokenStream, TokenTree};
use syn::parse::{Parse, ParseStream};
use wormx_core::TableRef;

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
    pub columns: Vec<ColumnMacroOptions>,
}

pub struct ColumnMacroOptions {
    /// The database column name.
    pub name: String,
    /// Optional Rust field name from `"column" as field`.
    pub field_name: Option<syn::Ident>,
    /// Field attributes, written without the surrounding `#[...]`.
    pub attrs: Vec<TokenStream>,
    pub span: Span,
}

impl Parse for TableMacroInput {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let literal: syn::LitStr = input.parse()?;
        let span = literal.span();
        let table = TableRef::parse(&literal.value())
            .map_err(|error| syn::Error::new(span, error.to_string()))?;

        let struct_name = if input.peek(syn::Token![as]) {
            input.parse::<syn::Token![as]>()?;
            Some(input.parse()?)
        } else {
            None
        };

        let mut derives = None;
        let mut attrs = Vec::new();
        let mut relation_attrs = Vec::new();
        let mut columns = Vec::new();

        if input.peek(syn::token::Brace) {
            let content;
            syn::braced!(content in input);

            while !content.is_empty() {
                let option: syn::Ident = content.parse()?;
                let option_name = option.to_string();

                match option_name.as_str() {
                    "derives" => {
                        let values;
                        syn::parenthesized!(values in content);
                        derives = Some(
                            values
                                .parse_terminated(syn::Path::parse, syn::Token![,])?
                                .into_iter()
                                .collect(),
                        );
                    }
                    "attrs" => attrs = parse_attrs(&content)?,
                    "relation_attrs" => relation_attrs = parse_attrs(&content)?,
                    "columns" => columns = parse_columns(&content)?,
                    _ => {
                        return Err(syn::Error::new(
                            option.span(),
                            format!(
                                "unknown `table!` option {option_name:?}; expected `derives`, \
                                 `attrs`, `relation_attrs`, or `columns`"
                            ),
                        ));
                    }
                }

                if content.peek(syn::Token![,]) {
                    content.parse::<syn::Token![,]>()?;
                } else if !content.is_empty() {
                    return Err(content.error("expected `,` between `table!` options"));
                }
            }
        }

        if !input.is_empty() {
            return Err(input.error(
                "expected a table name, an optional `as StructName`, and an optional \
                 `{ ... }` block, e.g. `table!(\"users\")` or \
                 `table!(\"users\" as User { columns { ... } })`",
            ));
        }

        Ok(Self {
            table,
            span,
            struct_name,
            derives,
            attrs,
            relation_attrs,
            columns,
        })
    }
}

fn parse_columns(input: ParseStream<'_>) -> syn::Result<Vec<ColumnMacroOptions>> {
    let content;
    syn::braced!(content in input);

    let mut columns = Vec::new();
    let mut seen = HashSet::new();

    while !content.is_empty() {
        let (name, span) = parse_column_name(&content)?;
        if !seen.insert(name.clone()) {
            return Err(syn::Error::new(
                span,
                format!("column {name:?} is configured more than once"),
            ));
        }

        let field_name = if content.peek(syn::Token![as]) {
            content.parse::<syn::Token![as]>()?;
            Some(content.parse()?)
        } else {
            None
        };

        let mut attrs = Vec::new();
        if content.peek(syn::token::Brace) {
            let options;
            syn::braced!(options in content);

            while !options.is_empty() {
                let option: syn::Ident = options.parse()?;
                let option_name = option.to_string();

                match option_name.as_str() {
                    "attrs" => attrs = parse_attrs(&options)?,
                    _ => {
                        return Err(syn::Error::new(
                            option.span(),
                            format!("unknown column option {option_name:?}; expected `attrs`"),
                        ));
                    }
                }

                if options.peek(syn::Token![,]) {
                    options.parse::<syn::Token![,]>()?;
                } else if !options.is_empty() {
                    return Err(options.error("expected `,` between column options"));
                }
            }
        }

        columns.push(ColumnMacroOptions {
            name,
            field_name,
            attrs,
            span,
        });

        if content.peek(syn::Token![,]) {
            content.parse::<syn::Token![,]>()?;
        } else if !content.is_empty() {
            return Err(content.error("expected `,` between columns"));
        }
    }

    Ok(columns)
}

fn parse_column_name(input: ParseStream<'_>) -> syn::Result<(String, Span)> {
    if input.peek(syn::LitStr) {
        let literal: syn::LitStr = input.parse()?;
        return Ok((literal.value(), literal.span()));
    }

    if input.peek(syn::Ident) {
        let ident: syn::Ident = input.parse()?;
        return Ok((unraw(&ident), ident.span()));
    }

    Err(input.error("expected a column name string or identifier"))
}

fn parse_attrs(input: ParseStream<'_>) -> syn::Result<Vec<TokenStream>> {
    let content;
    syn::parenthesized!(content in input);

    let mut attrs = Vec::new();
    while !content.is_empty() {
        let attr = parse_attribute(&content)?;
        if attr.is_empty() {
            return Err(content.error("expected an attribute"));
        }

        attrs.push(attr);

        if content.peek(syn::Token![,]) {
            content.parse::<syn::Token![,]>()?;
        } else if !content.is_empty() {
            return Err(content.error("expected `,` between attributes"));
        }
    }

    Ok(attrs)
}

/// Parses one attribute body, without the surrounding `#[...]`.
///
/// Attributes are captured as raw tokens rather than `syn::Meta`, because
/// tools such as utoipa accept values that are not Rust expressions, e.g.
/// `schema(value_type = Option<String>)`.
fn parse_attribute(input: ParseStream<'_>) -> syn::Result<TokenStream> {
    let mut tokens = TokenStream::new();

    while !input.is_empty() && !input.peek(syn::Token![,]) {
        let token: TokenTree = input.parse()?;
        tokens.extend(std::iter::once(token));
    }

    Ok(tokens)
}

fn unraw(ident: &syn::Ident) -> String {
    let name = ident.to_string();
    name.strip_prefix("r#").unwrap_or(&name).to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    fn parse(input: TokenStream) -> syn::Result<TableMacroInput> {
        syn::parse2(input)
    }

    #[test]
    fn parses_legacy_table_name() {
        let input = parse(quote!("public.users")).unwrap();

        assert_eq!(input.table.qualified_name(), "public.users");
        assert!(input.struct_name.is_none());
        assert!(input.derives.is_none());
        assert!(input.attrs.is_empty());
        assert!(input.relation_attrs.is_empty());
        assert!(input.columns.is_empty());
    }

    #[test]
    fn parses_table_options() {
        let input = parse(quote! {
            "dbo.source_file" as SourceFile {
                derives(Debug, Clone, serde::Serialize, utoipa::ToSchema),
                attrs(schema(as = TranscriptionError)),
                relation_attrs(serde(skip), schema(ignore)),
                columns {
                    publication_date {
                        attrs(
                            serde(serialize_with = "crate::helpers::time::serde_dt_opt::serialize"),
                            schema(value_type = Option<String>, format = DateTime),
                        ),
                    },
                    "type" as kind {
                        attrs(serde(rename = "type"), schema(rename = "type")),
                    },
                },
            }
        })
        .unwrap();

        assert_eq!(input.table.qualified_name(), "dbo.source_file");
        assert_eq!(input.struct_name.unwrap().to_string(), "SourceFile");

        let derives = input.derives.unwrap();
        let derive_names = derives
            .iter()
            .map(|derive| quote!(#derive).to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            derive_names,
            ["Debug", "Clone", "serde :: Serialize", "utoipa :: ToSchema"]
        );

        assert_eq!(input.attrs.len(), 1);
        assert_eq!(
            input.attrs[0].to_string(),
            "schema (as = TranscriptionError)"
        );

        assert_eq!(input.relation_attrs.len(), 2);
        assert_eq!(input.relation_attrs[0].to_string(), "serde (skip)");
        assert_eq!(input.relation_attrs[1].to_string(), "schema (ignore)");

        assert_eq!(input.columns.len(), 2);
        assert_eq!(input.columns[0].name, "publication_date");
        assert!(input.columns[0].field_name.is_none());
        assert_eq!(input.columns[0].attrs.len(), 2);
        assert!(
            input.columns[0].attrs[1]
                .to_string()
                .contains("Option < String >")
        );

        assert_eq!(input.columns[1].name, "type");
        assert_eq!(
            input.columns[1].field_name.as_ref().unwrap().to_string(),
            "kind"
        );
        assert_eq!(input.columns[1].attrs.len(), 2);
    }

    #[test]
    fn rejects_unknown_options() {
        let error = parse(quote!("users" { nope() }))
            .err()
            .expect("unknown option should fail");
        assert!(error.to_string().contains("unknown `table!` option"));
    }

    #[test]
    fn rejects_duplicate_columns() {
        let error = parse(quote! {
            "users" {
                columns {
                    name { attrs(allow(dead_code)) },
                    "name" as other_name,
                },
            }
        })
        .err()
        .expect("duplicate column should fail");
        assert!(error.to_string().contains("configured more than once"));
    }

    #[test]
    fn parses_raw_identifiers() {
        let input = parse(quote! {
            "users" {
                columns {
                    r#type as r#kind,
                },
            }
        })
        .unwrap();

        assert_eq!(input.columns[0].name, "type");
        assert_eq!(
            input.columns[0].field_name.as_ref().unwrap().to_string(),
            "r#kind"
        );
    }
}
