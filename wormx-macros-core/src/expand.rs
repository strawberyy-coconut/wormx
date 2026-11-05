use std::path::PathBuf;

use proc_macro2::TokenStream;
use wormx_core::Config;

use crate::cache;
use crate::driver::{TableDataSource, TableDriver};
use crate::env::{self, MacroEnv};
use crate::input::TableMacroInput;

pub fn expand_table(input: TableMacroInput, drivers: &[TableDriver]) -> syn::Result<TokenStream> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .map_err(|_| {
            syn::Error::new(
                input.span,
                "`CARGO_MANIFEST_DIR` is not set; wormx macros must be run by Cargo",
            )
        })?;

    let config = Config::load(&manifest_dir).map_err(|error| {
        syn::Error::new(input.span, format!("failed to load wormx config: {error}"))
    })?;

    let macro_env = MacroEnv::load(&manifest_dir, &config).map_err(|error| {
        syn::Error::new(
            input.span,
            format!("failed to load wormx environment: {error}"),
        )
    })?;

    let workspace_root = env::workspace_root(&manifest_dir).ok();
    let dirs = cache::cache_dirs(
        &config,
        macro_env.offline_dir.as_deref(),
        workspace_root.as_deref(),
    );

    // Cache-first by default. `WORMX_REDO_CACHE=1` skips this and goes to the database.
    if !macro_env.redo_cache {
        for driver in drivers {
            match cache::find_cached(&dirs, driver.db_name(), &input.table) {
                Ok(Some(data)) => {
                    return driver.expand(
                        &config,
                        input,
                        TableDataSource::Cached {
                            data: Box::new(data),
                        },
                        None,
                    );
                }
                Ok(None) => {}
                Err(error) => {
                    return Err(syn::Error::new(
                        input.span,
                        format!("failed to read wormx cache: {error}"),
                    ));
                }
            }
        }
    }

    if macro_env.offline {
        return Err(syn::Error::new(
            input.span,
            format!(
                "`WORMX_OFFLINE` is set, but there is no cached schema for table {}; \
                 run a build with `{}` set and without `WORMX_OFFLINE` to refresh .wormx",
                input.table, config.database_url_var
            ),
        ));
    }

    let Some(database_url) = macro_env.database_url.clone() else {
        return Err(syn::Error::new(
            input.span,
            format!(
                "no cached schema for table {} and `{}` is not set; \
                 set the database URL or provide a cache in .wormx",
                input.table, config.database_url_var
            ),
        ));
    };

    let driver = select_driver(&database_url, &config, drivers).ok_or_else(|| {
        let schemes = drivers
            .iter()
            .flat_map(|driver| driver.url_schemes().iter().copied())
            .collect::<Vec<_>>();
        syn::Error::new(
            input.span,
            format!(
                "no wormx driver matches database URL scheme {:?}; available schemes: {}",
                url_scheme(&database_url),
                if schemes.is_empty() {
                    "<none>".to_owned()
                } else {
                    schemes.join(", ")
                }
            ),
        )
    })?;

    let save_dir = macro_env
        .offline_dir
        .clone()
        .unwrap_or_else(|| config.cache_dir.clone());

    driver.expand(
        &config,
        input,
        TableDataSource::Live { database_url },
        Some(&save_dir),
    )
}

fn select_driver<'a>(
    database_url: &str,
    config: &Config,
    drivers: &'a [TableDriver],
) -> Option<&'a TableDriver> {
    if let Some(dialect) = &config.dialect {
        return drivers.iter().find(|driver| driver.matches_name(dialect));
    }

    let scheme = url_scheme(database_url);
    drivers.iter().find(|driver| driver.matches_scheme(scheme))
}

fn url_scheme(database_url: &str) -> &str {
    database_url
        .split_once("://")
        .map(|(scheme, _)| scheme)
        .unwrap_or(database_url)
}
