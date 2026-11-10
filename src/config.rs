use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::format::{Item, StrftimeItems};
use futures::channel::mpsc::{UnboundedReceiver, unbounded};
use notify::Watcher;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{error, info, warn};

pub const CONFIG_DIR_NAME: &str = "Dockwind";
pub const CONFIG_FILE_NAME: &str = "config.toml";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub appearance: Appearance,
    pub behavior: Behavior,
    pub clock: Clock,
    pub pinned: Vec<PinnedApp>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Appearance {
    pub background: String,
    pub corner_radius: u32,
    pub blur: BlurMode,
    pub icon_size: u32,
    pub padding: u32,
    pub margin_bottom: u32,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            background: "#1E1E2ECC".to_string(),
            corner_radius: 16,
            blur: BlurMode::None,
            icon_size: 40,
            padding: 8,
            margin_bottom: 12,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BlurMode {
    #[default]
    None,
    Mica,
    Acrylic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Behavior {
    pub auto_hide: bool,
    pub show_delay_ms: u64,
    pub hide_delay_ms: u64,
    pub animation_ms: u64,
    pub reserve_space: bool,
    pub hide_native_taskbar: bool,
    pub hide_in_fullscreen: bool,
    pub quit_hotkey: String,
}

impl Default for Behavior {
    fn default() -> Self {
        Self {
            auto_hide: true,
            show_delay_ms: 80,
            hide_delay_ms: 400,
            animation_ms: 180,
            reserve_space: false,
            hide_native_taskbar: false,
            hide_in_fullscreen: true,
            quit_hotkey: "Ctrl+Alt+Shift+Q".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Clock {
    pub time_format: String,
    pub date_format: String,
    pub locale: String,
}

impl Default for Clock {
    fn default() -> Self {
        Self {
            time_format: "%H:%M".to_string(),
            date_format: "%a, %d %b %Y".to_string(),
            locale: "en".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PinnedApp {
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct LoadedConfig {
    pub config: Config,
    pub warnings: Vec<String>,
}

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

    pub fn sanitize(mut self) -> LoadedConfig {
        let defaults = Config::default();
        let mut warnings = Vec::new();

        if !(16..=128).contains(&self.appearance.icon_size) {
            warnings.push(format!(
                "icon_size {} is outside 16..=128; using {}",
                self.appearance.icon_size, defaults.appearance.icon_size
            ));
            self.appearance.icon_size = defaults.appearance.icon_size;
        }

        let max_radius =
            (self.appearance.icon_size + self.appearance.padding.saturating_mul(2)) / 2;
        if self.appearance.corner_radius > max_radius {
            let replacement = defaults.appearance.corner_radius.min(max_radius);
            warnings.push(format!(
                "corner_radius {} exceeds half the dock height ({}); using {}",
                self.appearance.corner_radius, max_radius, replacement
            ));
            self.appearance.corner_radius = replacement;
        }

        if parse_rgba_hex(&self.appearance.background).is_none() {
            warnings.push(format!(
                "background '{}' is not #RRGGBB or #RRGGBBAA; using {}",
                self.appearance.background, defaults.appearance.background
            ));
            self.appearance.background = defaults.appearance.background;
        }

        if !valid_strftime(&self.clock.time_format) {
            warnings.push(format!(
                "time_format '{}' is invalid; using {}",
                self.clock.time_format, defaults.clock.time_format
            ));
            self.clock.time_format = defaults.clock.time_format.clone();
        }

        if !valid_strftime(&self.clock.date_format) {
            warnings.push(format!(
                "date_format '{}' is invalid; using {}",
                self.clock.date_format, defaults.clock.date_format
            ));
            self.clock.date_format = defaults.clock.date_format.clone();
        }

        LoadedConfig {
            config: self,
            warnings,
        }
    }
}

fn valid_strftime(value: &str) -> bool {
    StrftimeItems::new(value).all(|item| !matches!(item, Item::Error))
}

pub fn parse_rgba_hex(value: &str) -> Option<[u8; 4]> {
    let hex = value.strip_prefix('#')?;
    if !matches!(hex.len(), 6 | 8) || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }

    let red = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let green = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let blue = u8::from_str_radix(&hex[4..6], 16).ok()?;
    let alpha = if hex.len() == 8 {
        u8::from_str_radix(&hex[6..8], 16).ok()?
    } else {
        0xFF
    };

    Some([red, green, blue, alpha])
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

pub fn watch(path: &Path, initial: Config) -> Result<UnboundedReceiver<Config>, ConfigError> {
    let watch_path = path.to_path_buf();
    let (config_sender, config_receiver) = unbounded();
    let (sender, receiver) = std::sync::mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |event| {
        let _ = sender.send(event);
    })?;
    watcher.watch(path, notify::RecursiveMode::NonRecursive)?;

    std::thread::Builder::new()
        .name("config-watch".to_string())
        .spawn(move || {
            let _watcher = watcher;
            let mut last_valid = initial;

            while let Ok(event) = receiver.recv() {
                if let Err(event_error) = event {
                    warn!(%event_error, "config watcher event failed");
                }

                loop {
                    match receiver.recv_timeout(Duration::from_millis(200)) {
                        Ok(_) => continue,
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => break,
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
                    }
                }

                match Config::load_existing(&watch_path) {
                    Ok(loaded) => {
                        for warning in &loaded.warnings {
                            warn!("{warning}");
                        }
                        last_valid = loaded.config;
                        info!(config = ?last_valid, "config reloaded");
                        if config_sender.unbounded_send(last_valid.clone()).is_err() {
                            return;
                        }
                    }
                    Err(load_error) => {
                        error!(
                            %load_error,
                            config = ?last_valid,
                            "config reload failed; keeping the last valid config"
                        );
                    }
                }
            }
        })?;

    Ok(config_receiver)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rgb_and_rgba_hex() {
        assert_eq!(parse_rgba_hex("#1E1E2ECC"), Some([0x1E, 0x1E, 0x2E, 0xCC]));
        assert_eq!(parse_rgba_hex("#FF0000"), Some([0xFF, 0x00, 0x00, 0xFF]));
    }

    #[test]
    fn rejects_invalid_hex_colors() {
        for value in ["", "#", "#12345", "#GGGGGG", "1E1E2E", "#1234567"] {
            assert_eq!(parse_rgba_hex(value), None, "value={value}");
        }
    }

    #[test]
    fn default_config_round_trips_through_toml() {
        let default = Config::default();
        let encoded = toml::to_string_pretty(&default).expect("serialize default");
        let decoded: Config = toml::from_str(&encoded).expect("parse default");
        assert_eq!(decoded, default);
    }

    #[test]
    fn partial_config_keeps_defaults() {
        let config: Config = toml::from_str(
            r##"
            [appearance]
            background = "#FF000080"
            "##,
        )
        .expect("parse partial");
        assert_eq!(config.appearance.background, "#FF000080");
        assert_eq!(config.appearance.corner_radius, 16);
        assert_eq!(config.behavior.hide_delay_ms, 400);
        assert_eq!(config.clock.time_format, "%H:%M");
    }

    #[test]
    fn invalid_color_falls_back_with_warning() {
        let mut config = Config::default();
        config.appearance.background = "not-a-color".to_string();
        let loaded = config.sanitize();
        assert_eq!(
            loaded.config.appearance.background,
            Config::default().appearance.background
        );
        assert_eq!(loaded.warnings.len(), 1);
    }

    #[test]
    fn invalid_icon_size_falls_back_with_warning() {
        let mut config = Config::default();
        config.appearance.icon_size = 8;
        let loaded = config.sanitize();
        assert_eq!(loaded.config.appearance.icon_size, 40);
        assert_eq!(loaded.warnings.len(), 1);
    }

    #[test]
    fn radius_is_limited_to_half_the_dock_height() {
        let mut config = Config::default();
        config.appearance.icon_size = 16;
        config.appearance.padding = 0;
        config.appearance.corner_radius = 16;
        let loaded = config.sanitize();
        assert_eq!(loaded.config.appearance.corner_radius, 8);
        assert_eq!(loaded.warnings.len(), 1);
    }

    #[test]
    fn invalid_clock_formats_fall_back_with_warning() {
        let mut config = Config::default();
        config.clock.time_format = "%Q".to_string();
        config.clock.date_format = "%Q".to_string();
        let loaded = config.sanitize();
        assert_eq!(loaded.config.clock.time_format, "%H:%M");
        assert_eq!(loaded.config.clock.date_format, "%a, %d %b %Y");
        assert_eq!(loaded.warnings.len(), 2);
    }

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
