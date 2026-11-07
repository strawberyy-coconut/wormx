use std::collections::HashSet;

use proc_macro2::{Span, TokenStream, TokenTree};
use syn::parse::{Parse, ParseStream};
use wormx_core::{RelationMode, TableRef};

use super::{ColumnMacroOptions, TableMacroInput};

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
        let mut relations = None;
        let mut include_relations = Vec::new();
        let mut skip_relations = Vec::new();
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
                    "relations" => relations = Some(parse_relation_mode(&content)?),
                    "include_relations" => {
                        include_relations = parse_relation_names(&content)?;
                    }
                    "skip_relations" => {
                        skip_relations = parse_relation_names(&content)?;
                    }
                    "columns" => columns = parse_columns(&content)?,
                    _ => {
                        return Err(syn::Error::new(
                            option.span(),
                            format!(
                                "unknown `table!` option {option_name:?}; expected `derives`, \
                                 `attrs`, `relation_attrs`, `relations`, `include_relations`, \
                                 `skip_relations`, or `columns`"
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
            relations,
            include_relations,
            skip_relations,
            columns,
        })
    }
}

fn parse_relation_mode(input: ParseStream<'_>) -> syn::Result<RelationMode> {
    let content;
    syn::parenthesized!(content in input);

    let ident: syn::Ident = content.parse()?;
    let mode = ident
        .to_string()
        .parse()
        .map_err(|message| syn::Error::new(ident.span(), message))?;

    if !content.is_empty() {
        return Err(content.error("expected a single relation mode"));
    }

    Ok(mode)
}

fn parse_relation_names(input: ParseStream<'_>) -> syn::Result<Vec<String>> {
    let content;
    syn::parenthesized!(content in input);

    let mut names = Vec::new();
    while !content.is_empty() {
        if content.peek(syn::LitStr) {
            let literal: syn::LitStr = content.parse()?;
            names.push(literal.value());
        } else {
            let ident: syn::Ident = content.parse()?;
            names.push(unraw(&ident));
        }

        if content.peek(syn::Token![,]) {
            content.parse::<syn::Token![,]>()?;
        } else if !content.is_empty() {
            return Err(content.error("expected `,` between relation names"));
        }
    }

    Ok(names)
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
        let mut skip = false;
        if content.peek(syn::token::Brace) {
            let options;
            syn::braced!(options in content);

            while !options.is_empty() {
                let option: syn::Ident = options.parse()?;
                let option_name = option.to_string();

                match option_name.as_str() {
                    "attrs" => attrs = parse_attrs(&options)?,
                    "skip" => skip = true,
                    _ => {
                        return Err(syn::Error::new(
                            option.span(),
                            format!(
                                "unknown column option {option_name:?}; expected `attrs` or `skip`"
                            ),
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

        if skip && (field_name.is_some() || !attrs.is_empty()) {
            return Err(syn::Error::new(
                span,
                format!("column {name:?} is skipped; remove its `as` rename and `attrs` options"),
            ));
        }

        columns.push(ColumnMacroOptions {
            name,
            field_name,
            attrs,
            skip,
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
