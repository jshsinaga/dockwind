use chrono::format::{Item, StrftimeItems};

use super::model::{Config, LoadedConfig};

impl Config {
    pub fn sanitize(mut self) -> LoadedConfig {
        let defaults = Config::default();
        let mut warnings = Vec::new();

        if !(200..=4096).contains(&self.appearance.width) {
            warnings.push(format!(
                "width {} is outside 200..=4096; using {}",
                self.appearance.width, defaults.appearance.width
            ));
            self.appearance.width = defaults.appearance.width;
        }

        if !(48..=400).contains(&self.appearance.height) {
            warnings.push(format!(
                "height {} is outside 48..=400; using {}",
                self.appearance.height, defaults.appearance.height
            ));
            self.appearance.height = defaults.appearance.height;
        }

        if !(16..=128).contains(&self.appearance.icon_size) {
            warnings.push(format!(
                "icon_size {} is outside 16..=128; using {}",
                self.appearance.icon_size, defaults.appearance.icon_size
            ));
            self.appearance.icon_size = defaults.appearance.icon_size;
        }

        let max_icon_for_height = self.appearance.height.saturating_sub(16).max(16);
        if self.appearance.icon_size > max_icon_for_height {
            warnings.push(format!(
                "icon_size {} does not fit height {}; using {}",
                self.appearance.icon_size, self.appearance.height, max_icon_for_height
            ));
            self.appearance.icon_size = max_icon_for_height;
        }

        let paddings_fit = self
            .appearance
            .padding
            .checked_mul(2)
            .and_then(|padding| self.appearance.icon_size.checked_add(padding));
        if paddings_fit.is_none() {
            warnings.push(format!(
                "padding {} overflows the dock height with icon_size {}; using {}",
                self.appearance.padding, self.appearance.icon_size, defaults.appearance.padding
            ));
            self.appearance.padding = defaults.appearance.padding;
        }

        let max_padding = self
            .appearance
            .height
            .saturating_sub(self.appearance.icon_size)
            / 2;
        if self.appearance.padding > max_padding {
            warnings.push(format!(
                "padding {} does not fit height {} with icon_size {}; using {}",
                self.appearance.padding,
                self.appearance.height,
                self.appearance.icon_size,
                max_padding
            ));
            self.appearance.padding = max_padding;
        }

        if !(10..=100).contains(&self.appearance.opacity) {
            warnings.push(format!(
                "opacity {} is outside 10..=100; using {}",
                self.appearance.opacity, defaults.appearance.opacity
            ));
            self.appearance.opacity = defaults.appearance.opacity;
        }

        if self.appearance.offset > 512 {
            warnings.push(format!(
                "offset {} is greater than 512; using {}",
                self.appearance.offset, defaults.appearance.offset
            ));
            self.appearance.offset = defaults.appearance.offset;
        }

        let dock_height = self.appearance.icon_size + self.appearance.padding * 2;
        let max_radius = dock_height / 2;
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

        if self.appearance.border_width > 20 {
            warnings.push(format!(
                "border_width {} is greater than 20; using {}",
                self.appearance.border_width, defaults.appearance.border_width
            ));
            self.appearance.border_width = defaults.appearance.border_width;
        }

        if parse_rgba_hex(&self.appearance.border_color).is_none() {
            warnings.push(format!(
                "border_color '{}' is not #RRGGBB or #RRGGBBAA; using {}",
                self.appearance.border_color, defaults.appearance.border_color
            ));
            self.appearance.border_color = defaults.appearance.border_color;
        }

        if self.appearance.item_spacing > 64 {
            warnings.push(format!(
                "item_spacing {} is greater than 64; using {}",
                self.appearance.item_spacing, defaults.appearance.item_spacing
            ));
            self.appearance.item_spacing = defaults.appearance.item_spacing;
        }

        if !(1..=64).contains(&self.behavior.trigger_size) {
            warnings.push(format!(
                "trigger_size {} is outside 1..=64; using {}",
                self.behavior.trigger_size, defaults.behavior.trigger_size
            ));
            self.behavior.trigger_size = defaults.behavior.trigger_size;
        }

        if self.behavior.reserve_offset > 120 {
            warnings.push(format!(
                "reserve_offset {} is greater than 120; using {}",
                self.behavior.reserve_offset, defaults.behavior.reserve_offset
            ));
            self.behavior.reserve_offset = defaults.behavior.reserve_offset;
        }

        if !(-50..=50).contains(&self.clock.line_gap) {
            warnings.push(format!(
                "line_gap {} is outside -50..=50; using {}",
                self.clock.line_gap, defaults.clock.line_gap
            ));
            self.clock.line_gap = defaults.clock.line_gap;
        }

        if !(8..=72).contains(&self.clock.time_font_size) {
            warnings.push(format!(
                "time_font_size {} is outside 8..=72; using {}",
                self.clock.time_font_size, defaults.clock.time_font_size
            ));
            self.clock.time_font_size = defaults.clock.time_font_size;
        }

        if !(8..=48).contains(&self.clock.date_font_size) {
            warnings.push(format!(
                "date_font_size {} is outside 8..=48; using {}",
                self.clock.date_font_size, defaults.clock.date_font_size
            ));
            self.clock.date_font_size = defaults.clock.date_font_size;
        }

        if let Some(color) = self.clock.font_color.clone() {
            match parse_rgba_hex(&color) {
                Some(_) => {}
                None => {
                    warnings.push(format!(
                        "clock font_color '{color}' is not #RRGGBB or #RRGGBBAA; clearing"
                    ));
                    self.clock.font_color = None;
                }
            }
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
    fn overflowing_padding_falls_back_with_warning() {
        let mut config = Config::default();
        config.appearance.padding = u32::MAX;
        let loaded = config.sanitize();
        assert_eq!(loaded.config.appearance.padding, 8);
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
    fn invalid_size_values_fall_back_with_warnings() {
        let mut config = Config::default();
        config.appearance.width = 10;
        config.appearance.height = 1000;
        let loaded = config.sanitize();
        assert_eq!(loaded.config.appearance.width, 560);
        assert_eq!(loaded.config.appearance.height, 92);
        assert_eq!(loaded.warnings.len(), 2);
    }

    #[test]
    fn padding_is_capped_to_fit_the_height() {
        let mut config = Config::default();
        config.appearance.height = 64;
        config.appearance.icon_size = 40;
        config.appearance.padding = 20;
        let loaded = config.sanitize();
        assert_eq!(loaded.config.appearance.padding, 12);
        assert_eq!(loaded.config.appearance.height, 64);
        assert_eq!(loaded.warnings.len(), 1);
    }

    #[test]
    fn icon_is_capped_to_fit_the_height() {
        let mut config = Config::default();
        config.appearance.height = 64;
        config.appearance.icon_size = 96;
        config.appearance.padding = 0;
        let loaded = config.sanitize();
        assert_eq!(loaded.config.appearance.icon_size, 48);
        assert_eq!(loaded.config.appearance.height, 64);
        assert_eq!(loaded.warnings.len(), 1);
    }

    #[test]
    fn opacity_and_offset_are_bounded() {
        let mut config = Config::default();
        config.appearance.opacity = 5;
        config.appearance.offset = 900;
        let loaded = config.sanitize();
        assert_eq!(loaded.config.appearance.opacity, 100);
        assert_eq!(loaded.config.appearance.offset, 12);
        assert_eq!(loaded.warnings.len(), 2);
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
    fn invalid_border_and_item_spacing_fall_back_with_warnings() {
        let mut config = Config::default();
        config.appearance.border_width = 99;
        config.appearance.border_color = "not-a-color".to_string();
        config.appearance.item_spacing = 100;
        let loaded = config.sanitize();
        assert_eq!(
            loaded.config.appearance.border_width,
            Config::default().appearance.border_width
        );
        assert_eq!(
            loaded.config.appearance.border_color,
            Config::default().appearance.border_color
        );
        assert_eq!(
            loaded.config.appearance.item_spacing,
            Config::default().appearance.item_spacing
        );
        assert_eq!(loaded.warnings.len(), 3);
    }

    #[test]
    fn invalid_clock_styling_falls_back_with_warnings() {
        let mut config = Config::default();
        config.clock.line_gap = -100;
        config.clock.font_color = Some("invalid-hex".to_string());
        config.clock.time_font_size = 100;
        config.clock.date_font_size = 2;
        let loaded = config.sanitize();
        assert_eq!(
            loaded.config.clock.line_gap,
            Config::default().clock.line_gap
        );
        assert_eq!(loaded.config.clock.font_color, None);
        assert_eq!(
            loaded.config.clock.time_font_size,
            Config::default().clock.time_font_size
        );
        assert_eq!(
            loaded.config.clock.date_font_size,
            Config::default().clock.date_font_size
        );
        assert_eq!(loaded.warnings.len(), 4);
    }

    #[test]
    fn invalid_behavior_values_fall_back_with_warnings() {
        let mut config = Config::default();
        config.behavior.trigger_size = 0;
        config.behavior.reserve_offset = 200;
        let loaded = config.sanitize();
        assert_eq!(
            loaded.config.behavior.trigger_size,
            Config::default().behavior.trigger_size
        );
        assert_eq!(
            loaded.config.behavior.reserve_offset,
            Config::default().behavior.reserve_offset
        );
        assert_eq!(loaded.warnings.len(), 2);
    }
}
