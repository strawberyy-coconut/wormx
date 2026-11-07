use std::collections::HashSet;

use wormx_core::Config;

use crate::constraints::Constraints;
use crate::input::TableMacroInput;

use super::model::{CodegenPolicy, Field, Relation, RelationKind};
use super::naming::{
    column_accessor_ident, handle_ident, include_ident, incoming_relation_name,
    outgoing_relation_name, relation_ident, relation_target_type,
};
use super::policy::{relation_selected, table_column_skipped};

pub(super) fn collect_relations(
    config: &Config,
    input: &TableMacroInput,
    struct_name: &str,
    fields: &[Field],
    column_names: &HashSet<String>,
    constraints: &Constraints,
    policy: &CodegenPolicy,
) -> syn::Result<Vec<Relation>> {
    let mut candidates = Vec::new();
    let mut candidate_names = HashSet::new();
    let mut unavailable = std::collections::HashMap::new();
    let mut used_names = column_names.clone();

    for fk in &constraints.foreign_keys {
        if fk.columns.len() != fk.referenced_columns.len() || fk.columns.is_empty() {
            continue;
        }

        let name = outgoing_relation_name(fk, &mut used_names);
        candidate_names.insert(name.clone());
        if !relation_selected(policy, &name, RelationKind::BelongsTo) {
            continue;
        }

        let missing_source = missing_columns(fields, &fk.columns);
        let skipped_target = fk
            .referenced_columns
            .iter()
            .filter(|column| {
                table_column_skipped(config, input, policy, &fk.referenced_table, column)
            })
            .cloned()
            .collect::<Vec<_>>();
        if !missing_source.is_empty() || !skipped_target.is_empty() {
            unavailable.insert(
                name,
                unavailable_reason(&missing_source, &skipped_target, &[]),
            );
            continue;
        }

        let source_key_fields = fk
            .columns
            .iter()
            .map(|column| {
                fields
                    .iter()
                    .find(|field| field.db_name == *column)
                    .expect("missing columns were checked above")
                    .ident
                    .clone()
            })
            .collect::<Vec<_>>();
        let source_key_tys = fk
            .columns
            .iter()
            .map(|column| {
                fields
                    .iter()
                    .find(|field| field.db_name == *column)
                    .expect("missing columns were checked above")
                    .inner_ty
                    .clone()
            })
            .collect::<Vec<_>>();
        let source_key_nullable = fk
            .columns
            .iter()
            .map(|column| {
                fields
                    .iter()
                    .find(|field| field.db_name == *column)
                    .expect("missing columns were checked above")
                    .nullable
            })
            .collect::<Vec<_>>();
        let optional = source_key_nullable.iter().any(|value| *value);
        let target_ty =
            relation_target_type(config, input, struct_name, &fk.referenced_table, input.span)?;
        let storage_ty: syn::Type = if optional {
            let target = target_ty.clone();
            syn::parse_quote!(::std::option::Option<#target>)
        } else {
            target_ty.clone()
        };
        let include_ident = include_ident(struct_name, "BelongsTo", &name, input.span)?;
        let handle_ident = handle_ident(struct_name, &name, input.span)?;
        let target_key_accessors = fk
            .referenced_columns
            .iter()
            .map(|column| column_accessor_ident(column))
            .collect::<Vec<_>>();
        let target_pk_accessor = target_key_accessors.first().cloned();
        let key_ty = if source_key_tys.len() == 1 {
            source_key_tys[0].clone()
        } else {
            syn::parse_quote!((#(#source_key_tys),*))
        };

        candidates.push(Relation {
            kind: RelationKind::BelongsTo,
            field_ident: relation_ident(&name, input.span)?,
            field_name: name,
            storage_ty,
            target_ty,
            include_ident,
            handle_ident,
            fk_field: source_key_fields.first().cloned(),
            key_ty,
            key_nullable: optional,
            source_key_field: None,
            source_key_ty: None,
            target_fk_column: None,
            target_fk_accessor: None,
            target_fk_nullable: false,
            target_pk_accessor,
            target_table: fk.referenced_table.clone(),
            source_columns: fk.columns.clone(),
            target_columns: fk.referenced_columns.clone(),
            source_key_fields,
            source_key_tys,
            source_key_nullable,
            target_key_accessors,
            target_key_nullable: vec![false; fk.referenced_columns.len()],
        });
    }

    let mut incoming_counts = std::collections::HashMap::new();
    for incoming in &constraints.referenced_by {
        *incoming_counts
            .entry(incoming.source_table.qualified_name())
            .or_insert(0usize) += 1;
    }

    for incoming in &constraints.referenced_by {
        if incoming.columns.len() != incoming.referenced_columns.len()
            || incoming.columns.is_empty()
        {
            continue;
        }

        let count = incoming_counts
            .get(&incoming.source_table.qualified_name())
            .copied()
            .unwrap_or(1);
        let name = incoming_relation_name(incoming, count, &mut used_names);
        candidate_names.insert(name.clone());
        if !relation_selected(policy, &name, RelationKind::HasMany) {
            continue;
        }

        let missing_source = missing_columns(fields, &incoming.referenced_columns);
        let skipped_target = incoming
            .columns
            .iter()
            .filter(|column| {
                table_column_skipped(config, input, policy, &incoming.source_table, column)
            })
            .cloned()
            .collect::<Vec<_>>();
        if !missing_source.is_empty() || !skipped_target.is_empty() {
            unavailable.insert(
                name,
                unavailable_reason(&missing_source, &skipped_target, &[]),
            );
            continue;
        }

        let source_fields = incoming
            .referenced_columns
            .iter()
            .map(|column| {
                fields
                    .iter()
                    .find(|field| field.db_name == *column)
                    .expect("missing columns were checked above")
            })
            .collect::<Vec<_>>();
        if source_fields.iter().any(|field| field.nullable) {
            unavailable.insert(
                name,
                unavailable_reason(&[], &[], &incoming.referenced_columns),
            );
            continue;
        }

        let source_key_tys = source_fields
            .iter()
            .map(|field| field.inner_ty.clone())
            .collect::<Vec<_>>();
        let target_ty = relation_target_type(
            config,
            input,
            struct_name,
            &incoming.source_table,
            input.span,
        )?;
        let include_ident = include_ident(struct_name, "HasMany", &name, input.span)?;
        let handle_ident = handle_ident(struct_name, &name, input.span)?;
        let target_key_accessors = incoming
            .columns
            .iter()
            .map(|column| column_accessor_ident(column))
            .collect::<Vec<_>>();
        let key_ty = if source_key_tys.len() == 1 {
            source_key_tys[0].clone()
        } else {
            syn::parse_quote!((#(#source_key_tys),*))
        };

        candidates.push(Relation {
            kind: RelationKind::HasMany,
            field_ident: relation_ident(&name, input.span)?,
            field_name: name,
            storage_ty: syn::parse_quote!(::std::vec::Vec<#target_ty>),
            target_ty,
            include_ident,
            handle_ident,
            fk_field: None,
            key_ty,
            key_nullable: false,
            source_key_field: Some(source_fields[0].ident.clone()),
            source_key_ty: Some(source_key_tys[0].clone()),
            target_fk_column: Some(incoming.columns[0].clone()),
            target_fk_accessor: target_key_accessors.first().cloned(),
            target_fk_nullable: incoming.columns_nullable.iter().any(|nullable| *nullable),
            target_pk_accessor: None,
            target_table: incoming.source_table.clone(),
            source_columns: incoming.referenced_columns.clone(),
            target_columns: incoming.columns.clone(),
            source_key_fields: source_fields
                .iter()
                .map(|field| field.ident.clone())
                .collect(),
            source_key_tys,
            source_key_nullable: source_fields.iter().map(|field| field.nullable).collect(),
            target_key_accessors,
            target_key_nullable: incoming.columns_nullable.clone(),
        });
    }

    for name in &policy.include_relations {
        if !candidate_names.contains(name) {
            return Err(unknown_relation_error(
                input,
                &input.table.qualified_name(),
                name,
                &candidate_names,
            ));
        }
        if let Some(reason) = unavailable.get(name) {
            return Err(syn::Error::new(
                input.span,
                format!("cannot include relation {name:?}: {reason}"),
            ));
        }
    }

    for name in &policy.skip_relations {
        if !candidate_names.contains(name) {
            return Err(unknown_relation_error(
                input,
                &input.table.qualified_name(),
                name,
                &candidate_names,
            ));
        }
    }

    Ok(candidates)
}

pub(super) fn missing_columns(fields: &[Field], columns: &[String]) -> Vec<String> {
    columns
        .iter()
        .filter(|column| !fields.iter().any(|field| field.db_name == **column))
        .cloned()
        .collect()
}

pub(super) fn unavailable_reason(
    missing_source: &[String],
    skipped_target: &[String],
    nullable_source: &[String],
) -> String {
    let mut reasons = Vec::new();
    if !missing_source.is_empty() {
        reasons.push(format!(
            "source column(s) {} are skipped or unavailable",
            quoted_list(missing_source)
        ));
    }
    if !skipped_target.is_empty() {
        reasons.push(format!(
            "target column(s) {} are skipped",
            quoted_list(skipped_target)
        ));
    }
    if !nullable_source.is_empty() {
        reasons.push(format!(
            "referenced column(s) {} are nullable",
            quoted_list(nullable_source)
        ));
    }

    reasons.join("; ")
}

pub(super) fn quoted_list(columns: &[String]) -> String {
    columns
        .iter()
        .map(|column| format!("{column:?}"))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(super) fn unknown_relation_error(
    input: &TableMacroInput,
    table_name: &str,
    name: &str,
    candidate_names: &HashSet<String>,
) -> syn::Error {
    let mut known = candidate_names.iter().cloned().collect::<Vec<_>>();
    known.sort();

    syn::Error::new(
        input.span,
        format!(
            "table {table_name:?} has no relation {name:?}; known relations: {}",
            if known.is_empty() {
                "<none>".to_owned()
            } else {
                known.join(", ")
            }
        ),
    )
}
