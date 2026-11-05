use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use wormx_core::{Config, TableRef};

use crate::constraints::Constraints;

pub const CACHE_FORMAT: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedTable {
    pub format: u32,
    pub db_name: String,
    pub table: String,
    #[serde(default)]
    pub schema: Option<String>,
    pub describe: serde_json::Value,
    pub constraints: Constraints,
}

impl CachedTable {
    pub fn new(
        db_name: impl Into<String>,
        table: &TableRef,
        describe: serde_json::Value,
        constraints: Constraints,
    ) -> Self {
        Self {
            format: CACHE_FORMAT,
            db_name: db_name.into(),
            table: table.name.clone(),
            schema: table.schema.clone(),
            describe,
            constraints,
        }
    }

    pub fn table_ref(&self) -> TableRef {
        TableRef {
            schema: self.schema.clone(),
            name: self.table.clone(),
        }
    }
}

pub fn cache_file_name(db_name: &str, table: &TableRef) -> String {
    let mut hasher = Sha256::new();
    hasher.update(db_name.as_bytes());
    hasher.update([0]);
    if let Some(schema) = &table.schema {
        hasher.update(schema.as_bytes());
    }
    hasher.update([0]);
    hasher.update(table.name.as_bytes());
    format!("table-{}.json", hex::encode(hasher.finalize()))
}

pub fn cache_dirs(
    config: &Config,
    offline_dir: Option<&Path>,
    workspace_root: Option<&Path>,
) -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Some(dir) = offline_dir {
        dirs.push(dir.to_path_buf());
    }

    dirs.push(config.cache_dir.clone());

    if let Some(workspace_root) = workspace_root {
        let workspace_cache = workspace_root.join(".wormx");
        if workspace_cache != config.cache_dir {
            dirs.push(workspace_cache);
        }
    }

    dirs
}

pub fn find_cached(
    dirs: &[PathBuf],
    db_name: &str,
    table: &TableRef,
) -> Result<Option<CachedTable>, CacheError> {
    let file_name = cache_file_name(db_name, table);

    for dir in dirs {
        let path = dir.join(&file_name);
        match fs::read_to_string(&path) {
            Ok(contents) => {
                let data: CachedTable =
                    serde_json::from_str(&contents).map_err(|source| CacheError::Parse {
                        path: path.clone(),
                        source,
                    })?;
                if data.format != CACHE_FORMAT {
                    return Err(CacheError::Format {
                        path,
                        found: data.format,
                    });
                }
                if data.db_name != db_name
                    || data.table != table.name
                    || data.schema != table.schema
                {
                    return Err(CacheError::Mismatch { path });
                }
                return Ok(Some(data));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(source) => {
                return Err(CacheError::Io { path, source });
            }
        }
    }

    Ok(None)
}

pub fn write_cached(dir: &Path, data: &CachedTable) -> Result<PathBuf, CacheError> {
    fs::create_dir_all(dir).map_err(|source| CacheError::Io {
        path: dir.to_path_buf(),
        source,
    })?;

    let table = data.table_ref();
    let path = dir.join(cache_file_name(&data.db_name, &table));
    let tmp = dir.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("wormx-cache"),
        std::process::id()
    ));

    let mut bytes = serde_json::to_vec_pretty(data).map_err(CacheError::Serialize)?;
    bytes.push(b'\n');

    {
        let mut file = fs::File::create(&tmp).map_err(|source| CacheError::Io {
            path: tmp.clone(),
            source,
        })?;
        file.write_all(&bytes).map_err(|source| CacheError::Io {
            path: tmp.clone(),
            source,
        })?;
    }

    if path.exists() {
        let _ = fs::remove_file(&path);
    }

    fs::rename(&tmp, &path).map_err(|source| CacheError::Io {
        path: path.clone(),
        source,
    })?;
    Ok(path)
}

#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("failed to read cache file {path:?}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse cache file {path:?}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("cache file {path:?} uses format {found}, but this wormx version uses {CACHE_FORMAT}")]
    Format { path: PathBuf, found: u32 },
    #[error("cache file {path:?} does not match the requested table")]
    Mismatch { path: PathBuf },
    #[error("failed to serialize cache data: {0}")]
    Serialize(serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn writes_and_reads_cache() {
        let root = std::env::temp_dir().join(format!(
            "wormx-cache-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();

        let table = TableRef::parse("public.users").unwrap();
        let data = CachedTable::new(
            "PostgreSQL",
            &table,
            json!({"columns": []}),
            Constraints {
                primary_key: vec!["id".to_owned()],
                ..Constraints::default()
            },
        );

        let path = write_cached(&root, &data).unwrap();
        assert!(path.exists());
        assert!(
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("table-")
        );

        let found = find_cached(std::slice::from_ref(&root), "PostgreSQL", &table)
            .unwrap()
            .expect("cache entry");
        assert_eq!(found.db_name, "PostgreSQL");
        assert_eq!(found.constraints.primary_key, vec!["id"]);

        let _ = std::fs::remove_dir_all(&root);
    }
}
