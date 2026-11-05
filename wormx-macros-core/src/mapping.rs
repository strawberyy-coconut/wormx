use proc_macro2::Span;
use sqlx_core::type_checking::TypeChecking;
use sqlx_core::type_info::TypeInfo;
use wormx_core::{Config, TableRef};

pub fn rust_type_for<DB>(
    info: &DB::TypeInfo,
    config: &Config,
    table: &TableRef,
    column: &str,
    span: Span,
) -> syn::Result<syn::Type>
where
    DB: TypeChecking,
{
    if let Some(override_type) = column_override(config, table, column) {
        return parse_type(override_type).map_err(|error| {
            syn::Error::new(
                span,
                format!("invalid Rust type override for column {column:?}: {error}"),
            )
        });
    }

    if let Some(override_type) = type_override(config, table, info.name()) {
        return parse_type(override_type).map_err(|error| {
            syn::Error::new(
                span,
                format!(
                    "invalid Rust type override for SQL type {:?}: {error}",
                    info.name()
                ),
            )
        });
    }

    let preferred = config.sqlx_preferred_crates();
    match DB::return_type_for_id(info, &preferred) {
        Ok(rust_type) => parse_type(&rewrite_sqlx_path(rust_type)).map_err(|error| {
            syn::Error::new(
                span,
                format!(
                    "wormx produced an invalid Rust type for SQL type {:?}: {error}",
                    info.name()
                ),
            )
        }),
        Err(error) => {
            let hint = if let Some(feature) = DB::get_feature_gate(info) {
                format!(
                    "enable the `{feature}` Cargo feature on wormx, or add a `[macros.type-overrides]` entry in wormx.toml"
                )
            } else {
                "add a `[macros.type-overrides]` entry in wormx.toml".to_owned()
            };

            Err(syn::Error::new(
                span,
                format!(
                    "no Rust type mapping for SQL type {:?}: {error}; {hint}",
                    info.name()
                ),
            ))
        }
    }
}

fn column_override<'a>(config: &'a Config, table: &TableRef, column: &str) -> Option<&'a str> {
    for table_name in table_keys(table) {
        if let Some(value) = config.column_override(&table_name, column) {
            return Some(value);
        }
    }
    None
}

fn type_override<'a>(config: &'a Config, table: &TableRef, sql_type: &str) -> Option<&'a str> {
    let mut candidates = vec![sql_type.to_owned(), sql_type.to_ascii_uppercase()];

    if let Some(schema) = &table.schema {
        candidates.push(format!("{schema}.{sql_type}"));
        candidates.push(format!("{schema}.{}", sql_type.to_ascii_uppercase()));
    }

    candidates
        .iter()
        .find_map(|candidate| config.type_override(candidate))
}

fn table_keys(table: &TableRef) -> Vec<String> {
    let mut keys = Vec::with_capacity(2);
    if let Some(schema) = table.schema.as_deref() {
        keys.push(format!("{schema}.{}", table.name));
    }
    keys.push(table.name.clone());
    keys
}

fn parse_type(value: &str) -> syn::Result<syn::Type> {
    syn::parse_str(value)
}

fn rewrite_sqlx_path(value: &str) -> String {
    if let Some(rest) = value.strip_prefix("::sqlx::") {
        let rest = rest.replace("sqlx::", "::wormx::sqlx::");
        format!("::wormx::sqlx::{rest}")
    } else if let Some(rest) = value.strip_prefix("sqlx::") {
        let rest = rest.replace("sqlx::", "::wormx::sqlx::");
        format!("::wormx::sqlx::{rest}")
    } else {
        value.replace("sqlx::", "::wormx::sqlx::")
    }
}
