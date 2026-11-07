//! Code generation for the `table!` macro.

mod api;
mod composite;
mod include;
mod model;
mod naming;
mod policy;
mod relations;
mod row;
mod table;

#[cfg(test)]
mod tests;

use proc_macro2::TokenStream;
use quote::quote;
use sqlx_core::column::Column;
use sqlx_core::describe::Describe;
use sqlx_core::type_checking::TypeChecking;
use wormx_core::{Config, naming as core_naming};

use crate::constraints::Constraints;
use crate::input::TableMacroInput;

use policy::{codegen_policy, table_config};
use relations::collect_relations;

pub fn generate<DB>(
    config: &Config,
    input: &TableMacroInput,
    describe: &Describe<DB>,
    constraints: &Constraints,
) -> syn::Result<TokenStream>
where
    DB: TypeChecking,
{
    let table = &input.table;
    let table_name = table.qualified_name();
    let struct_name = input
        .struct_name
        .as_ref()
        .map(ToString::to_string)
        .or_else(|| {
            config
                .table_config(&table_name)
                .or_else(|| config.table_config(&table.name))
                .and_then(|table| table.struct_name.clone())
        })
        .unwrap_or_else(|| core_naming::struct_name(&table.name));

    let struct_ident: syn::Ident = syn::parse_str(&struct_name).map_err(|error| {
        syn::Error::new(
            input.span,
            format!("invalid struct name {struct_name:?}: {error}"),
        )
    })?;

    let policy = codegen_policy(config, input);
    if !policy.include_relations.is_empty()
        && policy.relations != wormx_core::RelationMode::Explicit
    {
        return Err(syn::Error::new(
            input.span,
            "`include_relations` / `include-relations` requires `relations = \"explicit\"`",
        ));
    }

    if let Some(table_config) = table_config(config, table) {
        for column in &table_config.skip_columns {
            if !describe
                .columns()
                .iter()
                .any(|described| described.name() == column)
            {
                return Err(syn::Error::new(
                    input.span,
                    format!(
                        "table {table_name:?} has no column {column:?} configured in \
                         `skip-columns`"
                    ),
                ));
            }
        }
    }

    let (fields, field_names) = row::collect_fields(config, input, table, describe, constraints)?;
    let relations = collect_relations(
        config,
        input,
        &struct_name,
        &fields,
        &field_names,
        constraints,
        &policy,
    )?;

    let visibility: syn::Visibility =
        syn::parse_str(&config.codegen.visibility).map_err(|error| {
            syn::Error::new(
                input.span,
                format!(
                    "invalid visibility {:?} in wormx config: {error}",
                    config.codegen.visibility
                ),
            )
        })?;

    let derives: Vec<syn::Path> = match &input.derives {
        Some(derives) => derives.clone(),
        None => config
            .codegen
            .derives
            .iter()
            .map(|derive| {
                syn::parse_str(derive).map_err(|error| {
                    syn::Error::new(
                        input.span,
                        format!("invalid derive path {derive:?} in wormx config: {error}"),
                    )
                })
            })
            .collect::<syn::Result<_>>()?,
    };

    let derive_attribute = if derives.is_empty() {
        TokenStream::new()
    } else {
        quote! { #[derive(#(#derives),*)] }
    };

    let row_tokens = row::quote_row(
        &struct_ident,
        &visibility,
        &derive_attribute,
        &input.attrs,
        &input.relation_attrs,
        &fields,
        &relations,
        &table_name,
    );

    let from_row_tokens = if config.codegen.from_row && !fields.is_empty() {
        row::quote_from_row(&struct_ident, &fields, &relations)
    } else {
        TokenStream::new()
    };

    let api_impl = api::quote_api_impl(
        &struct_ident,
        &struct_name,
        &visibility,
        &fields,
        &relations,
    )?;

    let include_impls = include::quote_include_impls(&struct_ident, &relations);
    let table_impl =
        table::quote_table_impl(table, &struct_ident, describe, constraints, &relations);
    let env_guards = table::quote_env_guards(config);

    Ok(quote! {
        #env_guards
        #row_tokens
        #from_row_tokens
        #api_impl
        #include_impls
        #table_impl
    })
}
