use proc_macro2::{Span, TokenStream};
use quote::quote;
use sqlx_core::column::Column;
use sqlx_core::describe::Describe;
use sqlx_core::type_checking::TypeChecking;
use sqlx_core::type_info::TypeInfo;
use wormx_core::{Config, TableRef};

use crate::constraints::Constraints;

use super::model::{Relation, RelationKind};

pub(super) fn quote_table_impl<DB>(
    table: &TableRef,
    struct_ident: &syn::Ident,
    describe: &Describe<DB>,
    constraints: &Constraints,
    relations: &[Relation],
) -> TokenStream
where
    DB: TypeChecking,
{
    let schema_tokens = match &table.schema {
        Some(schema) => {
            let schema = syn::LitStr::new(schema, Span::call_site());
            quote!(::std::option::Option::Some(#schema))
        }
        None => quote!(::std::option::Option::None),
    };

    let column_tokens = describe
        .columns()
        .iter()
        .enumerate()
        .map(|(index, column)| {
            let name = syn::LitStr::new(column.name(), Span::call_site());
            let sql_type = syn::LitStr::new(column.type_info().name(), Span::call_site());
            let nullable = describe.nullable(index).unwrap_or(true);
            let primary_key = constraints.is_primary_key(column.name());
            let has_default = constraints.has_default(column.name());
            let identity = constraints.is_identity(column.name());
            let generated = constraints.is_generated(column.name());
            quote! {
                ::wormx::ColumnDef {
                    name: #name,
                    sql_type: #sql_type,
                    nullable: #nullable,
                    primary_key: #primary_key,
                    has_default: #has_default,
                    identity: #identity,
                    generated: #generated,
                }
            }
        });

    let primary_key_tokens = constraints
        .primary_key
        .iter()
        .map(|column| syn::LitStr::new(column, Span::call_site()));

    let relation_tokens = relations.iter().map(|relation| {
        let name = syn::LitStr::new(&relation.field_name, Span::call_site());
        let kind = match relation.kind {
            RelationKind::BelongsTo => quote!(::wormx::RelationKind::BelongsTo),
            RelationKind::HasMany => quote!(::wormx::RelationKind::HasMany),
        };
        let target_table = syn::LitStr::new(&relation.target_table.name, Span::call_site());
        let target_schema = match &relation.target_table.schema {
            Some(schema) => {
                let schema = syn::LitStr::new(schema, Span::call_site());
                quote!(::std::option::Option::Some(#schema))
            }
            None => quote!(::std::option::Option::None),
        };
        let source_columns = relation
            .source_columns
            .iter()
            .map(|column| syn::LitStr::new(column, Span::call_site()));
        let target_columns = relation
            .target_columns
            .iter()
            .map(|column| syn::LitStr::new(column, Span::call_site()));
        quote! {
            ::wormx::RelationDef {
                name: #name,
                kind: #kind,
                target_table: #target_table,
                target_schema: #target_schema,
                source_columns: &[#(#source_columns),*],
                target_columns: &[#(#target_columns),*],
            }
        }
    });

    let table_name_lit = syn::LitStr::new(&table.name, Span::call_site());

    quote! {
        impl ::wormx::Table for #struct_ident {
            const DEF: &'static ::wormx::TableDef = &::wormx::TableDef {
                name: #table_name_lit,
                schema: #schema_tokens,
                columns: &[
                    #(#column_tokens),*
                ],
                primary_key: &[
                    #(#primary_key_tokens),*
                ],
                relations: &[
                    #(#relation_tokens),*
                ],
            };
        }
    }
}

pub(super) fn quote_env_guards(config: &Config) -> TokenStream {
    let database_url_var = syn::LitStr::new(&config.database_url_var, Span::call_site());
    let mut config_guards = TokenStream::new();
    for file in ["wormx.toml", "sqlx.toml"] {
        let path = config.manifest_dir.join(file);
        if let Some(path) = path.to_str().filter(|_| path.exists()) {
            let path = syn::LitStr::new(path, Span::call_site());
            config_guards.extend(quote! {
                const _: &'static str = ::std::include_str!(#path);
            });
        }
    }

    quote! {
        #config_guards
        const _: ::std::option::Option<&'static str> = ::std::option_env!(#database_url_var);
        const _: ::std::option::Option<&'static str> = ::std::option_env!("WORMX_OFFLINE");
        const _: ::std::option::Option<&'static str> = ::std::option_env!("WORMX_OFFLINE_DIR");
        const _: ::std::option::Option<&'static str> = ::std::option_env!("WORMX_REDO_CACHE");
    }
}
