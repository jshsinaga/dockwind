use serde::{Deserialize, Serialize};

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
