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
    #[serde(default)]
    pub padding_top: Option<u32>,
    #[serde(default)]
    pub padding_bottom: Option<u32>,
    #[serde(default)]
    pub padding_left: Option<u32>,
    #[serde(default)]
    pub padding_right: Option<u32>,
    pub border_color: String,
    pub border_width: u32,
    pub shadow: bool,
    pub item_spacing: u32,
    pub indicator_style: IndicatorStyle,
    pub item_alignment: ItemAlignment,
    pub alignment_anchor: AlignmentAnchor,
    pub width_mode: WidthMode,
    pub vertical_align: VerticalAlign,
    pub width: u32,
    pub height: u32,
    #[serde(alias = "margin_bottom")]
    pub offset: u32,
    pub orientation: DockEdge,
    /// Opacity percentage applied to the configured background alpha. 100 keeps
    /// the background exactly as written.
    pub opacity: u8,
}

impl Appearance {
    pub fn effective_padding(&self) -> (u32, u32, u32, u32) {
        (
            self.padding_top.unwrap_or(self.padding),
            self.padding_bottom.unwrap_or(self.padding),
            self.padding_left.unwrap_or(self.padding),
            self.padding_right.unwrap_or(self.padding),
        )
    }
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            background: "#1E1E2ECC".to_string(),
            corner_radius: 16,
            blur: BlurMode::None,
            icon_size: 40,
            padding: 8,
            padding_top: None,
            padding_bottom: None,
            padding_left: None,
            padding_right: None,
            border_color: "#FFFFFF33".to_string(),
            border_width: 1,
            shadow: true,
            item_spacing: 4,
            indicator_style: IndicatorStyle::Line,
            item_alignment: ItemAlignment::Center,
            alignment_anchor: AlignmentAnchor::Dock,
            width_mode: WidthMode::Manual,
            vertical_align: VerticalAlign::Center,
            width: 560,
            height: 92,
            offset: 12,
            orientation: DockEdge::Bottom,
            opacity: 100,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum IndicatorStyle {
    #[default]
    Line,
    Dot,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ItemAlignment {
    #[default]
    Center,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AlignmentAnchor {
    #[default]
    Dock,
    Clock,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WidthMode {
    #[default]
    Manual,
    Auto,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum VerticalAlign {
    #[default]
    Center,
    Top,
    Bottom,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ClockAlignment {
    #[default]
    Right,
    Center,
    Left,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DockEdge {
    #[default]
    Bottom,
    Top,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BlurMode {
    #[default]
    None,
    Mica,
    Acrylic,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AnimationCurve {
    #[default]
    EaseOut,
    EaseIn,
    EaseInOut,
    Linear,
    Ease,
    Bounce,
}

impl AnimationCurve {
    pub fn apply(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::EaseIn => t * t * t,
            Self::EaseOut => 1.0 - (1.0 - t).powi(3),
            Self::EaseInOut => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
                }
            }
            Self::Ease => t * t * (3.0 - 2.0 * t),
            Self::Bounce => {
                let c1 = 1.70158;
                let c3 = c1 + 1.0;
                1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Behavior {
    pub auto_hide: bool,
    pub show_delay_ms: u64,
    pub hide_delay_ms: u64,
    pub animation_ms: u64,
    pub animation_curve: AnimationCurve,
    pub trigger_size: u32,
    pub reserve_space: bool,
    pub reserve_offset: u32,
    pub hide_native_taskbar: bool,
    pub hide_in_fullscreen: bool,
    pub show_running_apps: bool,
    pub quit_hotkey: String,
}

impl Default for Behavior {
    fn default() -> Self {
        Self {
            auto_hide: true,
            show_delay_ms: 80,
            hide_delay_ms: 400,
            animation_ms: 180,
            animation_curve: AnimationCurve::EaseOut,
            trigger_size: 8,
            reserve_space: false,
            reserve_offset: 0,
            hide_native_taskbar: false,
            hide_in_fullscreen: true,
            show_running_apps: true,
            quit_hotkey: "Ctrl+Alt+Shift+Q".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Clock {
    pub show: bool,
    pub time_format: String,
    pub date_format: String,
    pub locale: String,
    pub line_gap: i32,
    pub font_family: String,
    pub font_color: Option<String>,
    pub time_font_size: u32,
    pub date_font_size: u32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub alignment: ClockAlignment,
    pub vertical_align: VerticalAlign,
}

impl Default for Clock {
    fn default() -> Self {
        Self {
            show: true,
            time_format: "%H:%M".to_string(),
            date_format: "%a, %d %b %Y".to_string(),
            locale: "en".to_string(),
            line_gap: 0,
            font_family: String::new(),
            font_color: None,
            time_font_size: 20,
            date_font_size: 12,
            bold: false,
            italic: false,
            underline: false,
            alignment: ClockAlignment::Right,
            vertical_align: VerticalAlign::Center,
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
        assert_eq!(config.appearance.width, 560);
        assert_eq!(config.appearance.orientation, DockEdge::Bottom);
        assert_eq!(config.behavior.hide_delay_ms, 400);
        assert_eq!(config.clock.time_format, "%H:%M");
    }

    #[test]
    fn legacy_margin_bottom_maps_to_offset() {
        let config: Config = toml::from_str(
            r##"
            [appearance]
            margin_bottom = 24
            "##,
        )
        .expect("parse legacy margin_bottom");
        assert_eq!(config.appearance.offset, 24);
    }

    #[test]
    fn orientation_parses_top_and_bottom() {
        let bottom: Config =
            toml::from_str("[appearance]\norientation = \"bottom\"\n").expect("bottom orientation");
        let top: Config =
            toml::from_str("[appearance]\norientation = \"top\"\n").expect("top orientation");
        assert_eq!(bottom.appearance.orientation, DockEdge::Bottom);
        assert_eq!(top.appearance.orientation, DockEdge::Top);
    }
}
