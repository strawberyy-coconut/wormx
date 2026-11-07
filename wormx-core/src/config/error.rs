use std::path::PathBuf;

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
