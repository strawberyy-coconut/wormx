use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use wormx_core::Config;

#[derive(Debug, Clone, Default)]
pub struct MacroEnv {
    pub database_url: Option<String>,
    pub offline: bool,
    pub offline_dir: Option<PathBuf>,
    pub redo_cache: bool,
}

impl MacroEnv {
    pub fn load(manifest_dir: &Path, config: &Config) -> Result<Self, EnvError> {
        let dotenv = load_dotenv_files(manifest_dir)?;

        let get = |name: &str| {
            std::env::var(name)
                .ok()
                .or_else(|| dotenv.get(name).cloned())
        };

        let database_url = get(&config.database_url_var).filter(|value| !value.is_empty());
        let offline = get("WORMX_OFFLINE")
            .as_deref()
            .map(is_truthy)
            .unwrap_or(false);
        let redo_cache = get("WORMX_REDO_CACHE")
            .as_deref()
            .map(is_truthy)
            .unwrap_or(false);
        let offline_dir = get("WORMX_OFFLINE_DIR").map(PathBuf::from);

        Ok(Self {
            database_url,
            offline,
            offline_dir,
            redo_cache,
        })
    }
}

fn load_dotenv_files(manifest_dir: &Path) -> Result<HashMap<String, String>, EnvError> {
    let mut values = HashMap::new();

    // Nearest `.env` wins.
    for dir in manifest_dir.ancestors() {
        let path = dir.join(".env");
        let iter = match dotenvy::from_path_iter(&path) {
            Ok(iter) => iter,
            Err(dotenvy::Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                continue;
            }
            Err(source) => {
                return Err(EnvError::Dotenv { path, source });
            }
        };

        for item in iter {
            let (name, value) = item.map_err(|source| EnvError::Dotenv {
                path: path.clone(),
                source,
            })?;
            values.entry(name).or_insert(value);
        }
    }

    Ok(values)
}

fn is_truthy(value: &str) -> bool {
    value.eq_ignore_ascii_case("true") || value == "1"
}

pub fn workspace_root(manifest_dir: &Path) -> Result<PathBuf, EnvError> {
    static CACHE: OnceLock<Result<PathBuf, String>> = OnceLock::new();

    CACHE
        .get_or_init(|| {
            let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
            let output = Command::new(cargo)
                .args(["metadata", "--format-version=1", "--no-deps"])
                .current_dir(manifest_dir)
                .env_remove("__CARGO_FIX_PLZ")
                .output()
                .map_err(|error| error.to_string())?;

            if !output.status.success() {
                return Err(format!(
                    "`cargo metadata` failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                ));
            }

            #[derive(serde::Deserialize)]
            struct CargoMetadata {
                workspace_root: PathBuf,
            }

            serde_json::from_slice::<CargoMetadata>(&output.stdout)
                .map(|metadata| metadata.workspace_root)
                .map_err(|error| error.to_string())
        })
        .clone()
        .map_err(EnvError::Metadata)
}

#[derive(Debug, thiserror::Error)]
pub enum EnvError {
    #[error("failed to read dotenv file {path:?}")]
    Dotenv {
        path: PathBuf,
        #[source]
        source: dotenvy::Error,
    },
    #[error("failed to resolve workspace root: {0}")]
    Metadata(String),
}
