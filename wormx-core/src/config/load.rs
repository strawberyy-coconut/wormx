use std::path::Path;

use sqlx_core::config::macros::{DateTimeCrate, NumericCrate, PreferredCrates};

use super::error::ConfigError;
use super::{CodegenConfig, Config, PreferredDateTime, PreferredNumeric, TableConfig, WormxToml};

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
