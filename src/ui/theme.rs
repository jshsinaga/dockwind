use anyhow::{Result, anyhow};
use gpui::{Pixels, Rgba, px, rgba};

use crate::config::{Appearance, parse_rgba_hex};

#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub background: Rgba,
    pub text: Rgba,
    pub running_indicator: Rgba,
    pub active_indicator: Rgba,
    pub radius: Pixels,
    pub padding: Pixels,
}

impl Theme {
    pub fn from_config(appearance: &Appearance) -> Result<Self> {
        let color = parse_rgba_hex(&appearance.background).ok_or_else(|| {
            anyhow!(
                "invalid background color '{}' passed to Theme::from_config",
                appearance.background
            )
        })?;
        let alpha = ((color[3] as f32 * appearance.opacity as f32) / 100.0)
            .round()
            .clamp(0.0, 255.0) as u8;
        let background = rgba(u32::from_be_bytes([color[0], color[1], color[2], alpha]));
        let text = if white_has_better_contrast([color[0], color[1], color[2]]) {
            rgba(0xFFFFFFFF)
        } else {
            rgba(0x000000FF)
        };

        Ok(Self {
            background,
            text,
            running_indicator: Rgba { a: 0.45, ..text },
            active_indicator: text,
            radius: px(appearance.corner_radius as f32),
            padding: px(appearance.padding as f32),
        })
    }
}

fn white_has_better_contrast(rgb: [u8; 3]) -> bool {
    let luminance = relative_luminance(rgb);
    contrast_ratio(1.0, luminance) >= contrast_ratio(luminance, 0.0)
}

fn relative_luminance(rgb: [u8; 3]) -> f64 {
    let [red, green, blue] = rgb.map(linearize);
    0.2126 * red + 0.7152 * green + 0.0722 * blue
}

fn linearize(channel: u8) -> f64 {
    let value = channel as f64 / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn contrast_ratio(luminance_a: f64, luminance_b: f64) -> f64 {
    let (lighter, darker) = if luminance_a >= luminance_b {
        (luminance_a, luminance_b)
    } else {
        (luminance_b, luminance_a)
    };
    (lighter + 0.05) / (darker + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn dark_background_uses_white_text() {
        let theme = Theme::from_config(&Config::default().appearance).expect("theme");
        assert_eq!(theme.text, rgba(0xFFFFFFFF));
    }

    #[test]
    fn light_background_uses_black_text() {
        let mut appearance = Config::default().appearance;
        appearance.background = "#FFFFFFFF".to_string();
        let theme = Theme::from_config(&appearance).expect("theme");
        assert_eq!(theme.text, rgba(0x000000FF));
    }

    #[test]
    fn mid_gray_uses_black_text() {
        let mut appearance = Config::default().appearance;
        appearance.background = "#808080FF".to_string();
        let theme = Theme::from_config(&appearance).expect("theme");
        assert_eq!(theme.text, rgba(0x000000FF));
    }

    #[test]
    fn theme_preserves_alpha_and_sizes() {
        let mut appearance = Config::default().appearance;
        appearance.background = "#FF000080".to_string();
        appearance.corner_radius = 24;
        appearance.padding = 10;
        let theme = Theme::from_config(&appearance).expect("theme");
        assert_eq!(theme.background, rgba(0xFF000080));
        assert_eq!(theme.radius, px(24.0));
        assert_eq!(theme.padding, px(10.0));
        assert_eq!(theme.running_indicator.a, 0.45);
    }

    #[test]
    fn opacity_scales_the_background_alpha() {
        let mut appearance = Config::default().appearance;
        appearance.background = "#102030FF".to_string();
        appearance.opacity = 50;
        let theme = Theme::from_config(&appearance).expect("theme");
        assert_eq!(theme.background, rgba(0x10203080));

        appearance.opacity = 100;
        let theme = Theme::from_config(&appearance).expect("theme");
        assert_eq!(theme.background, rgba(0x102030FF));
    }

    #[test]
    fn invalid_background_is_an_error() {
        let mut appearance = Config::default().appearance;
        appearance.background = "nope".to_string();
        assert!(Theme::from_config(&appearance).is_err());
    }
}
