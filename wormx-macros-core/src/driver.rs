use proc_macro2::TokenStream;
use serde::de::DeserializeOwned;
use sqlx_core::describe::Describe;
use sqlx_core::type_checking::TypeChecking;
use wormx_core::{Config, TableRef};

use crate::cache::{self, CachedTable};
use crate::constraints::{BoxDynError, Constraints};
use crate::generate;
use crate::input::TableMacroInput;

pub type ConstraintsFn = fn(&str, &TableRef, &Config) -> Result<Constraints, BoxDynError>;
pub type DescribeValueFn = fn(&TableRef, &str) -> Result<serde_json::Value, String>;

type ExpandFn = fn(
    DescribeValueFn,
    ConstraintsFn,
    &Config,
    TableMacroInput,
    TableDataSource,
    Option<&std::path::Path>,
) -> syn::Result<TokenStream>;

pub enum TableDataSource {
    Live { database_url: String },
    Cached { data: Box<CachedTable> },
}

pub struct TableDriver {
    db_name: &'static str,
    aliases: &'static [&'static str],
    url_schemes: &'static [&'static str],
    describe: DescribeValueFn,
    constraints: ConstraintsFn,
    expand: ExpandFn,
}

impl TableDriver {
    /// Creates a driver from describe and constraint functions.
    ///
    /// The describe function receives the table reference and database URL and
    /// returns serialized `Describe` metadata. The constraint function reads
    /// primary keys, defaults, identity/generated columns, and foreign keys.
    pub const fn new_with_fns<DB>(
        aliases: &'static [&'static str],
        describe: DescribeValueFn,
        constraints: ConstraintsFn,
    ) -> Self
    where
        DB: TypeChecking,
        Describe<DB>: DeserializeOwned,
    {
        Self {
            db_name: DB::NAME,
            aliases,
            url_schemes: DB::URL_SCHEMES,
            describe,
            constraints,
            expand: expand_with::<DB>,
        }
    }

    pub fn db_name(&self) -> &'static str {
        self.db_name
    }

    pub fn url_schemes(&self) -> &'static [&'static str] {
        self.url_schemes
    }

    pub fn matches_scheme(&self, scheme: &str) -> bool {
        self.url_schemes
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(scheme))
    }

    pub fn matches_name(&self, name: &str) -> bool {
        name.eq_ignore_ascii_case(self.db_name)
            || self
                .aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(name))
    }

    pub fn expand(
        &self,
        config: &Config,
        input: TableMacroInput,
        source: TableDataSource,
        save_dir: Option<&std::path::Path>,
    ) -> syn::Result<TokenStream> {
        (self.expand)(
            self.describe,
            self.constraints,
            config,
            input,
            source,
            save_dir,
        )
    }
}

fn expand_with<DB>(
    describe_fn: DescribeValueFn,
    constraints_fn: ConstraintsFn,
    config: &Config,
    input: TableMacroInput,
    source: TableDataSource,
    save_dir: Option<&std::path::Path>,
) -> syn::Result<TokenStream>
where
    DB: TypeChecking,
    Describe<DB>: DeserializeOwned,
{
    let table = input.table.clone();

    let (describe, constraints) = match source {
        TableDataSource::Live { database_url } => {
            let describe_value = describe_fn(&table, &database_url).map_err(|error| {
                syn::Error::new(
                    input.span,
                    format!("failed to describe table {table}: {error}"),
                )
            })?;

            let constraints = constraints_fn(&database_url, &table, config).map_err(|error| {
                syn::Error::new(
                    input.span,
                    format!("failed to read constraints for table {table}: {error}"),
                )
            })?;

            let data = CachedTable::new(
                DB::NAME,
                &table,
                describe_value.clone(),
                constraints.clone(),
            );

            if let Some(dir) = save_dir
                && let Err(error) = cache::write_cached(dir, &data)
            {
                return Err(syn::Error::new(
                    input.span,
                    format!("failed to write wormx cache: {error}"),
                ));
            }

            let describe: Describe<DB> =
                serde_json::from_value(describe_value).map_err(|error| {
                    syn::Error::new(
                        input.span,
                        format!("failed to decode schema for table {table}: {error}"),
                    )
                })?;

            (describe, constraints)
        }
        TableDataSource::Cached { data } => {
            let data = *data;
            if data.db_name != DB::NAME {
                return Err(syn::Error::new(
                    input.span,
                    format!(
                        "wormx cache contains data for {}, but {} was selected",
                        data.db_name,
                        DB::NAME
                    ),
                ));
            }

            let describe: Describe<DB> =
                serde_json::from_value(data.describe).map_err(|error| {
                    syn::Error::new(
                        input.span,
                        format!(
                            "failed to deserialize cached schema for table {table}: {error}; \
                             set `WORMX_REDO_CACHE=1` to refresh the cache"
                        ),
                    )
                })?;

            (describe, data.constraints)
        }
    };

    generate::generate::<DB>(config, &input, &describe, &constraints)
}
