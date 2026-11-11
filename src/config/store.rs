use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use thiserror::Error;
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table, value};

use super::model::{
    AlignmentAnchor, AnimationCurve, BlurMode, ClockAlignment, Config, DockEdge, IndicatorStyle,
    ItemAlignment, LoadedConfig, VerticalAlign, WidthMode,
};

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

    /// Writes a validated config while preserving comments, key ordering, and
    /// unknown keys in an existing valid TOML document. An invalid existing file
    /// is not overwritten.
    pub fn save_atomic_preserving(&self, path: &Path) -> Result<LoadedConfig, ConfigError> {
        let loaded = self.clone().sanitize();
        let contents = if path.exists() {
            let existing = fs::read_to_string(path).map_err(|source| ConfigError::Read {
                path: path.to_path_buf(),
                source,
            })?;
            let mut document: DocumentMut =
                existing
                    .parse::<DocumentMut>()
                    .map_err(|error| ConfigError::Parse {
                        path: path.to_path_buf(),
                        message: error.to_string(),
                    })?;
            update_document(&mut document, &loaded.config);
            document.to_string()
        } else {
            let body = toml::to_string_pretty(&loaded.config)?;
            format!(
                "# Dockwind configuration\n\
                 # Written by the Dockwind settings window. Edit and save to reload.\n\n{body}"
            )
        };
        write_atomic(path, &contents)?;
        Ok(loaded)
    }
}

fn write_atomic(path: &Path, contents: &str) -> Result<(), ConfigError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| ConfigError::Write {
            path: path.to_path_buf(),
            source,
        })?;
    }

    let temporary = path.with_extension("toml.tmp");
    let mut file = fs::File::create(&temporary).map_err(|source| ConfigError::Write {
        path: temporary.clone(),
        source,
    })?;
    let write_result = file
        .write_all(contents.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|source| ConfigError::Write {
            path: temporary.clone(),
            source,
        });
    drop(file);

    if let Err(error) = write_result {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }

    if let Err(source) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(ConfigError::Write {
            path: path.to_path_buf(),
            source,
        });
    }
    Ok(())
}

fn table_mut<'a>(document: &'a mut DocumentMut, key: &str) -> &'a mut Table {
    let item = document[key].or_insert(Item::Table(Table::new()));
    if !item.is_table() {
        *item = Item::Table(Table::new());
    }
    match item.as_table_mut() {
        Some(table) => table,
        None => unreachable!("the table was inserted above"),
    }
}

fn update_document(document: &mut DocumentMut, config: &Config) {
    let appearance = table_mut(document, "appearance");
    appearance["background"] = value(config.appearance.background.as_str());
    appearance["corner_radius"] = value(config.appearance.corner_radius as i64);
    appearance["blur"] = value(match config.appearance.blur {
        BlurMode::None => "none",
        BlurMode::Mica => "mica",
        BlurMode::Acrylic => "acrylic",
    });
    appearance["icon_size"] = value(config.appearance.icon_size as i64);
    appearance["padding"] = value(config.appearance.padding as i64);
    if let Some(val) = config.appearance.padding_top {
        appearance["padding_top"] = value(val as i64);
    } else {
        appearance.remove("padding_top");
    }
    if let Some(val) = config.appearance.padding_bottom {
        appearance["padding_bottom"] = value(val as i64);
    } else {
        appearance.remove("padding_bottom");
    }
    if let Some(val) = config.appearance.padding_left {
        appearance["padding_left"] = value(val as i64);
    } else {
        appearance.remove("padding_left");
    }
    if let Some(val) = config.appearance.padding_right {
        appearance["padding_right"] = value(val as i64);
    } else {
        appearance.remove("padding_right");
    }
    appearance["width"] = value(config.appearance.width as i64);
    appearance["height"] = value(config.appearance.height as i64);
    appearance["offset"] = value(config.appearance.offset as i64);
    appearance["orientation"] = value(match config.appearance.orientation {
        DockEdge::Bottom => "bottom",
        DockEdge::Top => "top",
    });
    appearance["opacity"] = value(config.appearance.opacity as i64);
    appearance["border_color"] = value(config.appearance.border_color.as_str());
    appearance["border_width"] = value(config.appearance.border_width as i64);
    appearance["shadow"] = value(config.appearance.shadow);
    appearance["item_spacing"] = value(config.appearance.item_spacing as i64);
    appearance["indicator_style"] = value(match config.appearance.indicator_style {
        IndicatorStyle::Line => "line",
        IndicatorStyle::Dot => "dot",
    });
    appearance["item_alignment"] = value(match config.appearance.item_alignment {
        ItemAlignment::Left => "left",
        ItemAlignment::Center => "center",
        ItemAlignment::Right => "right",
    });
    appearance["alignment_anchor"] = value(match config.appearance.alignment_anchor {
        AlignmentAnchor::Dock => "dock",
        AlignmentAnchor::Clock => "clock",
    });
    appearance["width_mode"] = value(match config.appearance.width_mode {
        WidthMode::Manual => "manual",
        WidthMode::Auto => "auto",
    });
    appearance["vertical_align"] = value(match config.appearance.vertical_align {
        VerticalAlign::Center => "center",
        VerticalAlign::Top => "top",
        VerticalAlign::Bottom => "bottom",
    });
    appearance.remove("margin_bottom");

    let behavior = table_mut(document, "behavior");
    behavior["auto_hide"] = value(config.behavior.auto_hide);
    behavior["show_delay_ms"] = value(config.behavior.show_delay_ms as i64);
    behavior["hide_delay_ms"] = value(config.behavior.hide_delay_ms as i64);
    behavior["animation_ms"] = value(config.behavior.animation_ms as i64);
    behavior["animation_curve"] = value(match config.behavior.animation_curve {
        AnimationCurve::EaseOut => "ease_out",
        AnimationCurve::EaseIn => "ease_in",
        AnimationCurve::EaseInOut => "ease_in_out",
        AnimationCurve::Linear => "linear",
        AnimationCurve::Ease => "ease",
        AnimationCurve::Bounce => "bounce",
    });
    behavior["trigger_size"] = value(config.behavior.trigger_size as i64);
    behavior["reserve_space"] = value(config.behavior.reserve_space);
    behavior["reserve_offset"] = value(config.behavior.reserve_offset as i64);
    behavior["hide_native_taskbar"] = value(config.behavior.hide_native_taskbar);
    behavior["hide_in_fullscreen"] = value(config.behavior.hide_in_fullscreen);
    behavior["show_running_apps"] = value(config.behavior.show_running_apps);
    behavior["quit_hotkey"] = value(config.behavior.quit_hotkey.as_str());

    let clock = table_mut(document, "clock");
    clock["show"] = value(config.clock.show);
    clock["time_format"] = value(config.clock.time_format.as_str());
    clock["date_format"] = value(config.clock.date_format.as_str());
    clock["locale"] = value(config.clock.locale.as_str());
    clock["line_gap"] = value(config.clock.line_gap as i64);
    clock["font_family"] = value(config.clock.font_family.as_str());
    if let Some(ref color) = config.clock.font_color {
        clock["font_color"] = value(color.as_str());
    } else {
        clock.remove("font_color");
    }
    clock["bold"] = value(config.clock.bold);
    clock["italic"] = value(config.clock.italic);
    clock["underline"] = value(config.clock.underline);
    clock["time_font_size"] = value(config.clock.time_font_size as i64);
    clock["date_font_size"] = value(config.clock.date_font_size as i64);
    clock["alignment"] = value(match config.clock.alignment {
        ClockAlignment::Right => "right",
        ClockAlignment::Center => "center",
        ClockAlignment::Left => "left",
    });
    clock["vertical_align"] = value(match config.clock.vertical_align {
        VerticalAlign::Center => "center",
        VerticalAlign::Top => "top",
        VerticalAlign::Bottom => "bottom",
    });

    let mut pinned = ArrayOfTables::new();
    for app in &config.pinned {
        let mut table = Table::new();
        table["name"] = value(app.name.as_str());
        table["path"] = value(app.path.as_str());
        let mut args = toml_edit::Array::new();
        for arg in &app.args {
            args.push(arg.as_str());
        }
        table["args"] = value(args);
        pinned.push(table);
    }
    document["pinned"] = Item::ArrayOfTables(pinned);
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

    #[test]
    fn save_preserving_round_trips_and_removes_the_temporary_file() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("config.toml");
        let mut config = Config::default();
        config.appearance.width = 720;
        config.appearance.orientation = crate::config::DockEdge::Top;

        let saved = config.save_atomic_preserving(&path).expect("save config");
        assert_eq!(saved.config, config);
        assert!(!directory.path().join("config.toml.tmp").exists());

        let loaded = Config::load_existing(&path).expect("reload saved config");
        assert_eq!(loaded.config, config);
    }

    #[test]
    fn save_atomic_preserving_keeps_comments_and_unknown_keys() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("config.toml");
        fs::write(
            &path,
            r##"# top comment
custom = "keep"

[appearance]
# width comment
width = 560

[unknown]
value = 7
"##,
        )
        .expect("write baseline");

        let mut config = Config::default();
        config.appearance.width = 700;
        config
            .save_atomic_preserving(&path)
            .expect("save preserved");

        let text = fs::read_to_string(&path).expect("read saved");
        assert!(text.contains("# top comment"));
        assert!(text.contains("custom = \"keep\""));
        assert!(text.contains("# width comment"));
        assert!(text.contains("[unknown]"));
        assert!(text.contains("width = 700"));

        let loaded = Config::load_existing(&path).expect("reload saved");
        assert_eq!(loaded.config.appearance.width, 700);
    }

    #[test]
    fn save_atomic_preserving_removes_legacy_alias_and_updates_pinned() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("config.toml");
        fs::write(&path, "pinned = []\n\n[appearance]\nmargin_bottom = 24\n")
            .expect("write baseline");

        let mut config = Config::default();
        config.appearance.offset = 12;
        config.pinned.push(crate::config::PinnedApp {
            name: "Notepad".to_string(),
            path: "C:\\Windows\\System32\\notepad.exe".to_string(),
            args: Vec::new(),
        });
        config
            .save_atomic_preserving(&path)
            .expect("save preserved");

        let text = fs::read_to_string(&path).expect("read saved");
        assert!(!text.contains("margin_bottom"));
        assert!(text.contains("offset = 12"));
        let loaded = Config::load_existing(&path).expect("reload saved");
        assert_eq!(loaded.config.pinned.len(), 1);
        assert_eq!(loaded.config.pinned[0].name, "Notepad");
    }

    #[test]
    fn save_atomic_preserving_rejects_invalid_existing_toml() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("config.toml");
        fs::write(&path, "not valid = [").expect("write invalid");

        assert!(matches!(
            Config::default().save_atomic_preserving(&path),
            Err(ConfigError::Parse { .. })
        ));
        assert_eq!(
            fs::read_to_string(&path).expect("read unchanged"),
            "not valid = ["
        );
    }

    #[test]
    fn save_preserving_writes_sanitized_values() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("config.toml");
        let mut config = Config::default();
        config.appearance.width = 10;
        config.appearance.opacity = 5;

        let saved = config.save_atomic_preserving(&path).expect("save config");
        assert_eq!(saved.config.appearance.width, 560);
        assert_eq!(saved.config.appearance.opacity, 100);
        assert_eq!(saved.warnings.len(), 2);

        let loaded = Config::load_existing(&path).expect("reload saved config");
        assert_eq!(loaded.config, saved.config);
    }

    #[test]
    fn save_atomic_preserving_writes_all_appearance_and_clock_fields() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("config.toml");
        let mut config = Config::default();
        config.appearance.padding_top = Some(14);
        config.appearance.padding_bottom = Some(16);
        config.appearance.padding_left = Some(18);
        config.appearance.padding_right = Some(20);
        config.appearance.border_color = "#123456".to_string();
        config.appearance.border_width = 3;
        config.appearance.shadow = true;
        config.appearance.item_spacing = 12;
        config.appearance.indicator_style = IndicatorStyle::Dot;
        config.appearance.item_alignment = ItemAlignment::Right;
        config.appearance.alignment_anchor = AlignmentAnchor::Dock;
        config.appearance.width_mode = WidthMode::Auto;
        config.appearance.vertical_align = VerticalAlign::Top;

        config.behavior.animation_curve = AnimationCurve::Bounce;
        config.behavior.trigger_size = 16;
        config.behavior.reserve_offset = 24;

        config.clock.show = true;
        config.clock.line_gap = -5;
        config.clock.font_family = "Cascadia Code".to_string();
        config.clock.font_color = Some("#ABCDEF".to_string());
        config.clock.bold = true;
        config.clock.italic = true;
        config.clock.underline = true;
        config.clock.time_font_size = 28;
        config.clock.date_font_size = 14;
        config.clock.alignment = ClockAlignment::Center;
        config.clock.vertical_align = VerticalAlign::Bottom;

        config
            .save_atomic_preserving(&path)
            .expect("save preserved");

        let loaded = Config::load_existing(&path).expect("reload saved");
        assert_eq!(loaded.config.appearance.padding_top, Some(14));
        assert_eq!(loaded.config.appearance.padding_bottom, Some(16));
        assert_eq!(loaded.config.appearance.padding_left, Some(18));
        assert_eq!(loaded.config.appearance.padding_right, Some(20));
        assert_eq!(loaded.config.appearance.border_color, "#123456");
        assert_eq!(loaded.config.appearance.border_width, 3);
        assert!(loaded.config.appearance.shadow);
        assert_eq!(loaded.config.appearance.item_spacing, 12);
        assert_eq!(
            loaded.config.appearance.indicator_style,
            IndicatorStyle::Dot
        );
        assert_eq!(
            loaded.config.appearance.item_alignment,
            ItemAlignment::Right
        );
        assert_eq!(
            loaded.config.appearance.alignment_anchor,
            AlignmentAnchor::Dock
        );
        assert_eq!(loaded.config.appearance.width_mode, WidthMode::Auto);
        assert_eq!(loaded.config.appearance.vertical_align, VerticalAlign::Top);

        assert_eq!(
            loaded.config.behavior.animation_curve,
            AnimationCurve::Bounce
        );
        assert_eq!(loaded.config.behavior.trigger_size, 16);
        assert_eq!(loaded.config.behavior.reserve_offset, 24);

        assert!(loaded.config.clock.show);
        assert_eq!(loaded.config.clock.line_gap, -5);
        assert_eq!(loaded.config.clock.font_family, "Cascadia Code");
        assert_eq!(loaded.config.clock.font_color, Some("#ABCDEF".to_string()));
        assert!(loaded.config.clock.bold);
        assert!(loaded.config.clock.italic);
        assert!(loaded.config.clock.underline);
        assert_eq!(loaded.config.clock.time_font_size, 28);
        assert_eq!(loaded.config.clock.date_font_size, 14);
        assert_eq!(loaded.config.clock.alignment, ClockAlignment::Center);
        assert_eq!(loaded.config.clock.vertical_align, VerticalAlign::Bottom);
    }
}
