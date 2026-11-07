use std::collections::HashSet;

use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, quote};
use sqlx_core::column::Column;
use sqlx_core::describe::Describe;
use sqlx_core::type_checking::TypeChecking;
use sqlx_core::type_info::TypeInfo;
use wormx_core::{Config, TableRef, naming};

use crate::constraints::Constraints;
use crate::input::TableMacroInput;
use crate::mapping::rust_type_for;

use super::model::{Field, Relation};
use super::policy::table_config;

pub(super) fn collect_fields<DB>(
    config: &Config,
    input: &TableMacroInput,
    table: &TableRef,
    describe: &Describe<DB>,
    constraints: &Constraints,
) -> syn::Result<(Vec<Field>, HashSet<String>)>
where
    DB: TypeChecking,
{
    let table_name = table.qualified_name();
    let mut fields = Vec::with_capacity(describe.columns().len());
    let mut field_names = HashSet::new();

    for (index, column) in describe.columns().iter().enumerate() {
        let db_name = column.name().to_owned();
        let column_options = input.columns.iter().find(|options| options.name == db_name);
        let config_skips_column = table_config(config, table)
            .is_some_and(|config| config.skip_columns.iter().any(|column| column == &db_name));
        let skipped = config_skips_column || column_options.is_some_and(|options| options.skip);

        if skipped {
            if constraints.is_primary_key(&db_name) {
                return Err(syn::Error::new(
                    input.span,
                    format!(
                        "cannot skip primary-key column {db_name:?} for table {table_name:?}; \
                         primary keys are required by `get(...)` and relation generation"
                    ),
                ));
            }

            if config_skips_column
                && column_options.is_some_and(|options| {
                    options.field_name.is_some() || !options.attrs.is_empty()
                })
            {
                return Err(syn::Error::new(
                    input.span,
                    format!(
                        "column {db_name:?} is skipped by `[wormx.tables] skip-columns`; remove \
                         its `columns` rename and attributes from `table!`"
                    ),
                ));
            }

            continue;
        }

        let field_ident = match column_options.and_then(|options| options.field_name.clone()) {
            Some(ident) => ident,
            None => {
                let field_name = naming::field_name(&db_name);
                syn::parse_str(&field_name).map_err(|error| {
                    syn::Error::new(
                        input.span,
                        format!(
                            "invalid field name {field_name:?} for column {db_name:?}: {error}"
                        ),
                    )
                })?
            }
        };
        let field_name = field_ident.to_string();
        let field_key = field_name
            .strip_prefix("r#")
            .unwrap_or(&field_name)
            .to_owned();

        if !field_names.insert(field_key.clone()) {
            return Err(syn::Error::new(
                input.span,
                format!(
                    "table {table_name:?} has multiple columns that map to Rust field {field_key:?}; \
                     rename one of them with `as` in `table!` or with `[macros.table-overrides]`"
                ),
            ));
        }

        let nullable = describe.nullable(index).unwrap_or(true);
        let inner_ty =
            rust_type_for::<DB>(column.type_info(), config, table, &db_name, input.span)?;
        let ty: syn::Type = if nullable {
            syn::parse_quote!(::std::option::Option<#inner_ty>)
        } else {
            inner_ty.clone()
        };

        let sql_type = column.type_info().name().to_owned();
        let primary_key = constraints.is_primary_key(&db_name);
        let has_default = constraints.has_default(&db_name);
        let identity = constraints.is_identity(&db_name);
        let generated = constraints.is_generated(&db_name);

        let mut doc = format!("SQL type: `{sql_type}`");
        if nullable {
            doc.push_str(", nullable");
        } else {
            doc.push_str(", not null");
        }
        if primary_key {
            doc.push_str(", primary key");
        }
        if has_default {
            if let Some(default) = constraints.defaults.get(&db_name) {
                doc.push_str(&format!(", default `{default}`"));
            } else {
                doc.push_str(", has default");
            }
        }
        if identity {
            doc.push_str(", identity");
        }
        if generated {
            doc.push_str(", generated");
        }

        fields.push(Field {
            ident: field_ident,
            db_name,
            inner_ty,
            ty,
            nullable,
            primary_key,
            doc,
            attrs: column_options
                .map(|options| options.attrs.clone())
                .unwrap_or_default(),
        });
    }

    for options in &input.columns {
        if !describe
            .columns()
            .iter()
            .any(|column| column.name() == options.name)
        {
            return Err(syn::Error::new(
                options.span,
                format!("table {table_name:?} has no column {:?}", options.name),
            ));
        }
    }

    Ok((fields, field_names))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn quote_row(
    struct_ident: &syn::Ident,
    visibility: &syn::Visibility,
    derive_attribute: &TokenStream,
    struct_attrs: &[TokenStream],
    relation_attrs: &[TokenStream],
    fields: &[Field],
    relations: &[Relation],
    table_name: &str,
) -> TokenStream {
    let struct_doc = syn::LitStr::new(
        &format!("Row type generated by `wormx::table!` for {table_name}."),
        Span::call_site(),
    );
    let field_tokens = fields.iter().map(|field| {
        let ident = &field.ident;
        let ty = &field.ty;
        let doc = syn::LitStr::new(&field.doc, Span::call_site());
        let attrs = &field.attrs;
        quote! {
            #[doc = #doc]
            #(#[#attrs])*
            pub #ident: #ty
        }
    });
    let relation_field_tokens = relations.iter().map(|relation| {
        let ident = &relation.field_ident;
        let ty = &relation.storage_ty;
        let doc = format!("Generated relation `{}`.", relation.field_name);
        quote! {
            #[doc = #doc]
            #(#[#relation_attrs])*
            #ident: ::wormx::Deferred<#ty>
        }
    });

    quote! {
        #[doc = #struct_doc]
        #derive_attribute
        #(#[#struct_attrs])*
        #visibility struct #struct_ident {
            #(#field_tokens,)*
            #(#relation_field_tokens,)*
        }
    }
}

pub(super) fn quote_from_row(
    struct_ident: &syn::Ident,
    fields: &[Field],
    relations: &[Relation],
) -> TokenStream {
    let mut seen = HashSet::new();
    let bounds = fields.iter().filter_map(|field| {
        let ty = &field.ty;
        let key = ty.to_token_stream().to_string();
        if !seen.insert(key) {
            return None;
        }
        Some(quote! {
            #ty: ::wormx::__private::Decode<
                    'r,
                    <__WormxRow as ::wormx::__private::Row>::Database
                >
                + ::wormx::__private::Type<
                    <__WormxRow as ::wormx::__private::Row>::Database
                >
        })
    });

    let init = fields.iter().map(|field| {
        let ident = &field.ident;
        let ty = &field.ty;
        let db_name = syn::LitStr::new(&field.db_name, Span::call_site());
        quote! {
            #ident: ::wormx::__private::Row::try_get::<#ty, _>(__row, #db_name)?
        }
    });
    let relation_init = relations.iter().map(|relation| {
        let ident = &relation.field_ident;
        quote!(#ident: ::wormx::Deferred::unloaded())
    });

    quote! {
        impl<'r, __WormxRow> ::wormx::__private::FromRow<'r, __WormxRow> for #struct_ident
        where
            __WormxRow: ::wormx::__private::Row,
            for<'index> &'index str:
                ::wormx::__private::ColumnIndex<__WormxRow>,
            #(#bounds),*
        {
            fn from_row(
                __row: &'r __WormxRow,
            ) -> ::std::result::Result<Self, ::wormx::__private::Error> {
                ::std::result::Result::Ok(Self {
                    #(#init,)*
                    #(#relation_init,)*
                })
            }
        }
    }
}
