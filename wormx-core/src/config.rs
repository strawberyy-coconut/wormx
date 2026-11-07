//! Configuration loaded from `wormx.toml` and `sqlx.toml`.

mod error;
mod load;
#[cfg(test)]
mod tests;

pub use self::error::ConfigError;

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Deserialize;

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
