use std::fs;
use std::path::{Path, PathBuf};

use thiserror::Error;

use super::model::{Config, LoadedConfig};

pub const CONFIG_DIR_NAME: &str = "Dockwind";
pub const CONFIG_FILE_NAME: &str = "config.toml";

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("no user config directory is available")]
    NoConfigDir,
    #[error("failed to read config {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to write config {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to serialize default config: {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error("failed to parse config {path}: {message}")]
    Parse { path: PathBuf, message: String },
    #[error("failed to watch config: {0}")]
    Watch(#[from] notify::Error),
    #[error("failed to start config watcher thread: {0}")]
    WatchThread(#[from] std::io::Error),
    #[error("failed to resolve config path {path}: {source}")]
    Resolve {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("config watcher thread panicked")]
    WatcherPanicked,
}

pub fn default_path() -> Result<PathBuf, ConfigError> {
    let config_dir = dirs::config_dir().ok_or(ConfigError::NoConfigDir)?;
    Ok(config_dir.join(CONFIG_DIR_NAME).join(CONFIG_FILE_NAME))
}

impl Config {
    pub fn load_or_create(path: &Path) -> Result<LoadedConfig, ConfigError> {
        if !path.exists() {
            let config = Config::default();
            write_default(path, &config)?;
            return Ok(LoadedConfig {
                config,
                warnings: Vec::new(),
            });
        }

        match Self::load_existing(path) {
            Ok(loaded) => Ok(loaded),
            Err(ConfigError::Parse { message, .. }) => Ok(LoadedConfig {
                config: Config::default(),
                warnings: vec![format!(
                    "config parse failed; using defaults until the file is fixed: {message}"
                )],
            }),
            Err(other) => Err(other),
        }
    }

    pub fn load_existing(path: &Path) -> Result<LoadedConfig, ConfigError> {
        let contents = fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        let config: Config = toml::from_str(&contents).map_err(|error| ConfigError::Parse {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
        Ok(config.sanitize())
    }
}

fn write_default(path: &Path, config: &Config) -> Result<(), ConfigError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| ConfigError::Write {
            path: path.to_path_buf(),
            source,
        })?;
    }

    let body = toml::to_string_pretty(config)?;
    let contents = format!(
        "# Dockwind configuration\n\
         # Generated on first run. Edit and save to reload.\n\n{body}"
    );
    fs::write(path, contents).map_err(|source| ConfigError::Write {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_is_created_with_defaults() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("Dockwind").join("config.toml");
        let loaded = Config::load_or_create(&path).expect("load default");
        assert_eq!(loaded.config, Config::default());
        assert!(loaded.warnings.is_empty());
        assert!(path.exists());
    }

    #[test]
    fn corrupt_file_uses_defaults_at_startup() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("config.toml");
        fs::write(&path, "not valid toml = [").expect("write corrupt");
        let loaded = Config::load_or_create(&path).expect("load corrupt");
        assert_eq!(loaded.config, Config::default());
        assert_eq!(loaded.warnings.len(), 1);
    }

    #[test]
    fn corrupt_file_is_an_error_for_reload() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("config.toml");
        fs::write(&path, "not valid toml = [").expect("write corrupt");
        assert!(matches!(
            Config::load_existing(&path),
            Err(ConfigError::Parse { .. })
        ));
    }
}
