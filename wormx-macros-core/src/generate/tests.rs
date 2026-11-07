use quote::quote;
use wormx_core::{Config, RelationMode};

use crate::input::TableMacroInput;

use super::policy::codegen_policy;

#[test]
fn relation_policy_merges_config_and_macro_options() {
    let mut config = Config::default();
    config.codegen.relations = RelationMode::Outgoing;
    config.tables.insert(
        "public.users".to_owned(),
        wormx_core::TableConfig {
            skip_columns: vec!["password_hash".to_owned()],
            relations: Some(RelationMode::Explicit),
            include_relations: vec!["blogs".to_owned()],
            skip_relations: vec!["audit_logs".to_owned()],
            ..Default::default()
        },
    );

    let input: TableMacroInput = syn::parse2(quote! {
        "public.users" {
            relations(none),
            include_relations(posts),
            skip_relations(likes),
            columns {
                "avatar" { skip },
            },
        }
    })
    .unwrap();

    let policy = codegen_policy(&config, &input);

    assert_eq!(policy.relations, RelationMode::None);
    assert!(policy.skip_columns.contains("password_hash"));
    assert!(policy.skip_columns.contains("avatar"));
    assert!(policy.include_relations.contains("blogs"));
    assert!(policy.include_relations.contains("posts"));
    assert!(policy.skip_relations.contains("audit_logs"));
    assert!(policy.skip_relations.contains("likes"));
}
