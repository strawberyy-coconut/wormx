use std::collections::HashSet;

use heck::ToUpperCamelCase;
use proc_macro2::Span;
use wormx_core::{Config, TableRef, naming};

use crate::constraints::{ForeignKey, IncomingForeignKey};
use crate::input::TableMacroInput;

pub(super) fn outgoing_relation_name(fk: &ForeignKey, used: &mut HashSet<String>) -> String {
    let base = if fk.columns.len() > 1 {
        fk.referenced_table.name.clone()
    } else {
        fk.columns
            .first()
            .and_then(|column| column.strip_suffix("_id"))
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| fk.referenced_table.name.clone())
    };

    unique_name(&naming::field_name(&base), used)
}

pub(super) fn incoming_relation_name(
    incoming: &IncomingForeignKey,
    count: usize,
    used: &mut HashSet<String>,
) -> String {
    let base = if count > 1 {
        let fk = incoming
            .columns
            .first()
            .map(|column| column.strip_suffix("_id").unwrap_or(column))
            .filter(|column| !column.is_empty())
            .unwrap_or("source");
        format!("{fk}_{}", incoming.source_table.name)
    } else {
        incoming.source_table.name.clone()
    };

    unique_name(&naming::field_name(&base), used)
}

pub(super) fn unique_name(base: &str, used: &mut HashSet<String>) -> String {
    if used.insert(base.to_owned()) {
        return base.to_owned();
    }

    let mut index = 2;
    loop {
        let candidate = format!("{base}_{index}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
        index += 1;
    }
}

pub(super) fn relation_ident(name: &str, span: Span) -> syn::Result<syn::Ident> {
    syn::parse_str(name).map_err(|error| {
        syn::Error::new(
            span,
            format!("invalid relation field name {name:?}: {error}"),
        )
    })
}

/// A hidden accessor for a column, keyed by its database name.
///
/// Relation code is generated in a different `table!` invocation than the
/// target row type, so it cannot know whether the target column's field was
/// renamed with `as`. Encoding the database name keeps both sides in agreement:
/// the target generates the accessor, and the source calls it by column name.
pub(super) fn column_accessor_ident(column: &str) -> syn::Ident {
    syn::Ident::new(
        &format!("__wormx_column_{}", hex::encode(column.as_bytes())),
        Span::call_site(),
    )
}

pub(super) fn include_ident(
    struct_name: &str,
    kind: &str,
    name: &str,
    span: Span,
) -> syn::Result<syn::Ident> {
    let name = format!(
        "{}{}{}Include",
        struct_name.trim_start_matches("r#"),
        kind,
        name.trim_start_matches("r#").to_upper_camel_case()
    );
    syn::parse_str(&name).map_err(|error| {
        syn::Error::new(span, format!("invalid include type name {name:?}: {error}"))
    })
}

pub(super) fn handle_ident(struct_name: &str, name: &str, span: Span) -> syn::Result<syn::Ident> {
    let name = format!(
        "{}Relation{}",
        struct_name.trim_start_matches("r#"),
        name.trim_start_matches("r#").to_upper_camel_case()
    );
    syn::parse_str(&name).map_err(|error| {
        syn::Error::new(
            span,
            format!("invalid relation type name {name:?}: {error}"),
        )
    })
}

pub(super) fn relation_target_type(
    config: &Config,
    input: &TableMacroInput,
    struct_name: &str,
    table: &TableRef,
    span: Span,
) -> syn::Result<syn::Type> {
    if table == &input.table {
        return syn::parse_str(struct_name).map_err(|error| {
            syn::Error::new(
                span,
                format!("invalid self-relation struct name {struct_name:?}: {error}"),
            )
        });
    }

    target_type(config, table, span)
}

pub(super) fn target_type(config: &Config, table: &TableRef, span: Span) -> syn::Result<syn::Type> {
    let table_config = config
        .table_config(&table.qualified_name())
        .or_else(|| config.table_config(&table.name));

    if let Some(path) = table_config.and_then(|config| config.rust_path.as_deref()) {
        return syn::parse_str(path).map_err(|error| {
            syn::Error::new(
                span,
                format!("invalid rust-path {path:?} for table {table}: {error}"),
            )
        });
    }

    let name = table_config
        .and_then(|config| config.struct_name.clone())
        .unwrap_or_else(|| naming::struct_name(&table.name));

    syn::parse_str(&name).map_err(|error| {
        syn::Error::new(
            span,
            format!("invalid target struct name {name:?} for table {table}: {error}"),
        )
    })
}

pub(super) fn relation_key_ident(index: usize) -> syn::Ident {
    syn::Ident::new(&format!("key_{index}"), Span::call_site())
}
