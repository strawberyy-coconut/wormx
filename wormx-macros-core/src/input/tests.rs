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
    assert!(input.relations.is_none());
    assert!(input.include_relations.is_empty());
    assert!(input.skip_relations.is_empty());
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
fn parses_skip_and_relation_options() {
    let input = parse(quote! {
        "public.users" as User {
            relations(explicit),
            include_relations(blogs, "posts"),
            skip_relations(audit_logs),
            columns {
                "password_hash" { skip },
                name,
            },
        }
    })
    .unwrap();

    assert_eq!(input.relations, Some(RelationMode::Explicit));
    assert_eq!(input.include_relations, ["blogs", "posts"]);
    assert_eq!(input.skip_relations, ["audit_logs"]);
    assert!(input.columns[0].skip);
    assert!(!input.columns[1].skip);
}

#[test]
fn rejects_skip_with_rename_or_attrs() {
    let error = parse(quote! {
        "users" {
            columns {
                "name" as display_name { skip },
            },
        }
    })
    .err()
    .expect("skip with rename should fail");
    assert!(error.to_string().contains("is skipped"));

    let error = parse(quote! {
        "users" {
            columns {
                "name" { skip, attrs(doc = "name") },
            },
        }
    })
    .err()
    .expect("skip with attrs should fail");
    assert!(error.to_string().contains("is skipped"));
}

#[test]
fn rejects_invalid_relation_mode() {
    let error = parse(quote!("users" { relations(sometimes) }))
        .err()
        .expect("invalid relation mode should fail");
    assert!(error.to_string().contains("invalid relation mode"));
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
