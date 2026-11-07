//! Configuration loaded from `wormx.toml` and `sqlx.toml`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use sqlx_core::config::macros::{DateTimeCrate, NumericCrate, PreferredCrates};

/// The contents of a `wormx.toml` file.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct WormxToml {
    pub common: CommonToml,
    pub macros: MacrosToml,
    pub wormx: WormxSectionToml,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
/// Shared settings that `wormx.toml` and `sqlx.toml` read.
pub struct CommonToml {
    /// Environment variable that holds the database URL.
    pub database_url_var: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
/// Macro-specific configuration.
pub struct MacrosToml {
    pub preferred_crates: PreferredCratesToml,
    pub type_overrides: BTreeMap<String, String>,
    pub table_overrides: BTreeMap<String, BTreeMap<String, String>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
/// Preferred third-party crates for SQL types.
pub struct PreferredCratesToml {
    pub date_time: Option<String>,
    pub numeric: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
/// The `[wormx]` section.
pub struct WormxSectionToml {
    pub cache_dir: Option<String>,
    /// Force a dialect instead of inferring it from the URL scheme.
    pub dialect: Option<String>,
    pub database: DatabaseToml,
    pub codegen: CodegenToml,
    pub tables: BTreeMap<String, TableToml>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
/// Database connection settings.
pub struct DatabaseToml {
    pub url_var: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
/// Generated-code settings.
pub struct CodegenToml {
    pub derives: Option<Vec<String>>,
    pub visibility: Option<String>,
    pub from_row: Option<bool>,
    pub relations: Option<RelationMode>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
/// Per-table code generation overrides.
pub struct TableToml {
    #[serde(rename = "struct")]
    pub struct_name: Option<String>,
    pub rust_path: Option<String>,
    pub skip_columns: Vec<String>,
    pub relations: Option<RelationMode>,
    pub include_relations: Vec<String>,
    pub skip_relations: Vec<String>,
}

/// Which foreign-key relations `table!` generates for a row type.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RelationMode {
    /// Generate outgoing `belongs_to` and incoming `has_many` relations.
    #[default]
    Auto,
    /// Generate only outgoing `belongs_to` relations.
    Outgoing,
    /// Generate only incoming `has_many` relations.
    Incoming,
    /// Generate only the relations named by `include-relations`.
    Explicit,
    /// Generate no relations.
    None,
}

impl std::str::FromStr for RelationMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "auto" => Ok(Self::Auto),
            "outgoing" => Ok(Self::Outgoing),
            "incoming" => Ok(Self::Incoming),
            "explicit" => Ok(Self::Explicit),
            "none" => Ok(Self::None),
            _ => Err(format!(
                "invalid relation mode {value:?}; expected `auto`, `outgoing`, `incoming`, \
                 `explicit`, or `none`"
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
/// The preferred crate for date/time types.
pub enum PreferredDateTime {
    #[default]
    Inferred,
    Chrono,
    Time,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
/// The preferred crate for numeric types.
pub enum PreferredNumeric {
    #[default]
    Inferred,
    BigDecimal,
    RustDecimal,
}

#[derive(Debug, Clone)]
/// Resolved code generation settings.
pub struct CodegenConfig {
    pub derives: Vec<String>,
    pub visibility: String,
    pub from_row: bool,
    pub relations: RelationMode,
}

impl Default for CodegenConfig {
    fn default() -> Self {
        Self {
            derives: vec!["Debug".to_owned(), "Clone".to_owned()],
            visibility: "pub".to_owned(),
            from_row: true,
            relations: RelationMode::Auto,
        }
    }
}

#[derive(Debug, Clone, Default)]
/// Resolved per-table settings.
pub struct TableConfig {
    pub struct_name: Option<String>,
    pub rust_path: Option<String>,
    pub skip_columns: Vec<String>,
    pub relations: Option<RelationMode>,
    pub include_relations: Vec<String>,
    pub skip_relations: Vec<String>,
}

/// Merged configuration from `wormx.toml` and, when present, `sqlx.toml`.
#[derive(Debug, Clone)]
pub struct Config {
    pub manifest_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub database_url_var: String,
    pub dialect: Option<String>,
    pub type_overrides: BTreeMap<String, String>,
    pub table_overrides: BTreeMap<String, BTreeMap<String, String>>,
    pub preferred_date_time: PreferredDateTime,
    pub preferred_numeric: PreferredNumeric,
    pub codegen: CodegenConfig,
    pub tables: BTreeMap<String, TableConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            manifest_dir: PathBuf::new(),
            cache_dir: PathBuf::from(".wormx"),
            database_url_var: "DATABASE_URL".to_owned(),
            dialect: None,
            type_overrides: BTreeMap::new(),
            table_overrides: BTreeMap::new(),
            preferred_date_time: PreferredDateTime::Inferred,
            preferred_numeric: PreferredNumeric::Inferred,
            codegen: CodegenConfig::default(),
            tables: BTreeMap::new(),
        }
    }
}

impl Config {
    /// Loads `wormx.toml` and `sqlx.toml` from the crate manifest directory.
    pub fn load(manifest_dir: &Path) -> Result<Self, ConfigError> {
        let wormx = read_optional_toml(&manifest_dir.join("wormx.toml"))?;
        let sqlx = read_optional_toml(&manifest_dir.join("sqlx.toml"))?;

        let mut config = Config {
            manifest_dir: manifest_dir.to_path_buf(),
            ..Config::default()
        };

        let database_url_var = wormx
            .wormx
            .database
            .url_var
            .clone()
            .or_else(|| wormx.common.database_url_var.clone())
            .or_else(|| sqlx.common.database_url_var.clone())
            .unwrap_or_else(|| "DATABASE_URL".to_owned());

        config.database_url_var = database_url_var;
        config.dialect = wormx.wormx.dialect.clone();

        let cache_dir = wormx
            .wormx
            .cache_dir
            .clone()
            .unwrap_or_else(|| ".wormx".to_owned());
        config.cache_dir = manifest_dir.join(cache_dir);

        // sqlx.toml is the fallback; wormx.toml wins on conflicts.
        config
            .type_overrides
            .extend(sqlx.macros.type_overrides.clone());
        config
            .type_overrides
            .extend(wormx.macros.type_overrides.clone());

        // Merge per column: wormx.toml wins for conflicting columns, but keys
        // that only appear in sqlx.toml are preserved.
        for (table, columns) in &sqlx.macros.table_overrides {
            config
                .table_overrides
                .entry(table.clone())
                .or_default()
                .extend(columns.clone());
        }
        for (table, columns) in &wormx.macros.table_overrides {
            config
                .table_overrides
                .entry(table.clone())
                .or_default()
                .extend(columns.clone());
        }

        config.preferred_date_time = parse_date_time(
            wormx.macros.preferred_crates.date_time.as_deref().or(sqlx
                .macros
                .preferred_crates
                .date_time
                .as_deref()),
        )?;
        config.preferred_numeric = parse_numeric(
            wormx.macros.preferred_crates.numeric.as_deref().or(sqlx
                .macros
                .preferred_crates
                .numeric
                .as_deref()),
        )?;

        config.codegen = CodegenConfig {
            derives: wormx
                .wormx
                .codegen
                .derives
                .clone()
                .unwrap_or_else(|| vec!["Debug".to_owned(), "Clone".to_owned()]),
            visibility: wormx
                .wormx
                .codegen
                .visibility
                .clone()
                .unwrap_or_else(|| "pub".to_owned()),
            from_row: wormx.wormx.codegen.from_row.unwrap_or(true),
            relations: wormx.wormx.codegen.relations.unwrap_or_default(),
        };

        for (table, table_config) in &wormx.wormx.tables {
            config.tables.insert(
                table.clone(),
                TableConfig {
                    struct_name: table_config.struct_name.clone(),
                    rust_path: table_config.rust_path.clone(),
                    skip_columns: table_config.skip_columns.clone(),
                    relations: table_config.relations,
                    include_relations: table_config.include_relations.clone(),
                    skip_relations: table_config.skip_relations.clone(),
                },
            );
        }

        Ok(config)
    }

    pub fn table_config(&self, table: &str) -> Option<&TableConfig> {
        self.tables.get(table)
    }

    pub fn type_override(&self, type_name: &str) -> Option<&str> {
        self.type_overrides.get(type_name).map(String::as_str)
    }

    pub fn column_override(&self, table: &str, column: &str) -> Option<&str> {
        self.table_overrides
            .get(table)
            .and_then(|columns| columns.get(column))
            .map(String::as_str)
    }

    pub fn sqlx_preferred_crates(&self) -> PreferredCrates {
        PreferredCrates {
            date_time: match self.preferred_date_time {
                PreferredDateTime::Inferred => DateTimeCrate::Inferred,
                PreferredDateTime::Chrono => DateTimeCrate::Chrono,
                PreferredDateTime::Time => DateTimeCrate::Time,
            },
            numeric: match self.preferred_numeric {
                PreferredNumeric::Inferred => NumericCrate::Inferred,
                PreferredNumeric::BigDecimal => NumericCrate::BigDecimal,
                PreferredNumeric::RustDecimal => NumericCrate::RustDecimal,
            },
        }
    }
}

fn read_optional_toml(path: &Path) -> Result<WormxToml, ConfigError> {
    match std::fs::read_to_string(path) {
        Ok(contents) => toml::from_str(&contents).map_err(|source| ConfigError::Parse {
            path: path.to_path_buf(),
            source,
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(WormxToml::default()),
        Err(source) => Err(ConfigError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn parse_date_time(value: Option<&str>) -> Result<PreferredDateTime, ConfigError> {
    match value.map(|value| value.trim().to_ascii_lowercase()) {
        None => Ok(PreferredDateTime::Inferred),
        Some(value) if value == "inferred" => Ok(PreferredDateTime::Inferred),
        Some(value) if value == "chrono" => Ok(PreferredDateTime::Chrono),
        Some(value) if value == "time" => Ok(PreferredDateTime::Time),
        Some(value) => Err(ConfigError::InvalidPreferredCrate {
            option: "date-time",
            value,
        }),
    }
}

fn parse_numeric(value: Option<&str>) -> Result<PreferredNumeric, ConfigError> {
    match value.map(|value| value.trim().to_ascii_lowercase()) {
        None => Ok(PreferredNumeric::Inferred),
        Some(value) if value == "inferred" => Ok(PreferredNumeric::Inferred),
        Some(value) if value == "bigdecimal" => Ok(PreferredNumeric::BigDecimal),
        Some(value) if value == "rust_decimal" || value == "rust-decimal" => {
            Ok(PreferredNumeric::RustDecimal)
        }
        Some(value) => Err(ConfigError::InvalidPreferredCrate {
            option: "numeric",
            value,
        }),
    }
}

#[derive(Debug, thiserror::Error)]
/// Errors from loading wormx configuration.
pub enum ConfigError {
    #[error("failed to read config file {path:?}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse config file {path:?}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("invalid preferred crate {value:?} for `{option}` in wormx config")]
    InvalidPreferredCrate { option: &'static str, value: String },
}

#[cfg(test)]
mod tests {
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
}
