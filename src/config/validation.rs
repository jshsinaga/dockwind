use chrono::format::{Item, StrftimeItems};

use super::model::{Config, LoadedConfig};

impl Config {
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
    fn invalid_clock_formats_fall_back_with_warning() {
        let mut config = Config::default();
        config.clock.time_format = "%Q".to_string();
        config.clock.date_format = "%Q".to_string();
        let loaded = config.sanitize();
        assert_eq!(loaded.config.clock.time_format, "%H:%M");
        assert_eq!(loaded.config.clock.date_format, "%a, %d %b %Y");
        assert_eq!(loaded.warnings.len(), 2);
    }
}
