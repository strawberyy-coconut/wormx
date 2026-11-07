use super::*;

#[test]
fn parses_relation_and_column_policy() {
    let root = temp_dir("relation-policy");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("wormx.toml"),
        r#"
            [wormx.codegen]
            relations = "outgoing"

            [wormx.tables."public.users"]
            skip-columns = ["password_hash"]
            relations = "explicit"
            include-relations = ["blogs"]
            skip-relations = ["audit_logs"]
            "#,
    )
    .unwrap();

    let config = Config::load(&root).unwrap();
    assert_eq!(config.codegen.relations, RelationMode::Outgoing);
    let users = config.table_config("public.users").unwrap();
    assert_eq!(users.skip_columns, ["password_hash"]);
    assert_eq!(users.relations, Some(RelationMode::Explicit));
    assert_eq!(users.include_relations, ["blogs"]);
    assert_eq!(users.skip_relations, ["audit_logs"]);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn rejects_invalid_relation_mode() {
    let root = temp_dir("invalid-relation-mode");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("wormx.toml"),
        r#"
            [wormx.codegen]
            relations = "sometimes"
            "#,
    )
    .unwrap();

    let error = Config::load(&root).expect_err("invalid relation mode should fail");
    assert!(error.to_string().contains("failed to parse config file"));

    let _ = std::fs::remove_dir_all(&root);
}

fn temp_dir(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "wormx-config-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

#[test]
fn merges_sqlx_and_wormx_config() {
    let root = std::env::temp_dir().join(format!(
        "wormx-config-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();

    std::fs::write(
        root.join("sqlx.toml"),
        r#"
            [common]
            database-url-var = "SQLX_DB"

            [macros.preferred-crates]
            date-time = "chrono"

            [macros.type-overrides]
            "uuid" = "uuid::Uuid"

            [macros.table-overrides.users]
            name = "String"
            email = "String"
            "#,
    )
    .unwrap();

    std::fs::write(
        root.join("wormx.toml"),
        r#"
            [wormx]
            cache-dir = "target/wormx-cache"

            [wormx.tables.users]
            struct = "User"

            [macros.type-overrides]
            "mood" = "crate::Mood"

            [macros.table-overrides.users]
            name = "crate::Name"
            "#,
    )
    .unwrap();

    let config = Config::load(&root).unwrap();
    assert_eq!(config.database_url_var, "SQLX_DB");
    assert_eq!(config.preferred_date_time, PreferredDateTime::Chrono);
    assert_eq!(config.type_override("uuid"), Some("uuid::Uuid"));
    assert_eq!(config.type_override("mood"), Some("crate::Mood"));
    assert_eq!(config.column_override("users", "name"), Some("crate::Name"));
    assert_eq!(config.column_override("users", "email"), Some("String"));
    assert_eq!(
        config.tables.get("users").unwrap().struct_name.as_deref(),
        Some("User")
    );
    assert_eq!(config.cache_dir, root.join("target/wormx-cache"));

    let _ = std::fs::remove_dir_all(&root);
}
