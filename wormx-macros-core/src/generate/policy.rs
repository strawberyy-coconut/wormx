use std::collections::HashSet;

use wormx_core::{Config, RelationMode, TableRef};

use crate::input::TableMacroInput;

use super::model::{CodegenPolicy, RelationKind};

pub(super) fn table_config<'a>(
    config: &'a Config,
    table: &TableRef,
) -> Option<&'a wormx_core::TableConfig> {
    config
        .table_config(&table.qualified_name())
        .or_else(|| config.table_config(&table.name))
}

pub(super) fn codegen_policy(config: &Config, input: &TableMacroInput) -> CodegenPolicy {
    let table_config = table_config(config, &input.table);
    let relations = input
        .relations
        .or_else(|| table_config.and_then(|config| config.relations))
        .unwrap_or(config.codegen.relations);

    let mut skip_columns: HashSet<String> = table_config
        .map(|config| config.skip_columns.iter().cloned().collect())
        .unwrap_or_default();
    skip_columns.extend(
        input
            .columns
            .iter()
            .filter(|column| column.skip)
            .map(|column| column.name.clone()),
    );

    let mut include_relations: HashSet<String> = table_config
        .map(|config| {
            config
                .include_relations
                .iter()
                .cloned()
                .collect::<HashSet<_>>()
        })
        .unwrap_or_default();
    include_relations.extend(input.include_relations.iter().cloned());

    let mut skip_relations: HashSet<String> = table_config
        .map(|config| config.skip_relations.iter().cloned().collect())
        .unwrap_or_default();
    skip_relations.extend(input.skip_relations.iter().cloned());

    CodegenPolicy {
        relations,
        include_relations,
        skip_relations,
        skip_columns,
    }
}

pub(super) fn relation_selected(policy: &CodegenPolicy, name: &str, kind: RelationKind) -> bool {
    if policy.skip_relations.contains(name) {
        return false;
    }

    match policy.relations {
        RelationMode::Auto => true,
        RelationMode::Outgoing => kind == RelationKind::BelongsTo,
        RelationMode::Incoming => kind == RelationKind::HasMany,
        RelationMode::Explicit => policy.include_relations.contains(name),
        RelationMode::None => false,
    }
}

pub(super) fn table_column_skipped(
    config: &Config,
    input: &TableMacroInput,
    policy: &CodegenPolicy,
    table: &TableRef,
    column: &str,
) -> bool {
    if table == &input.table {
        return policy.skip_columns.contains(column);
    }

    let Some(table_config) = table_config(config, table) else {
        return false;
    };

    table_config
        .skip_columns
        .iter()
        .any(|skipped| skipped == column)
}
