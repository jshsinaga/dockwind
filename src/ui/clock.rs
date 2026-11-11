use std::time::Duration;

use chrono::{Local, Timelike};
use gpui::{
    Context, FontWeight, Render, SharedString, Task, Timer, Window, div, prelude::*, px, rgba,
};

use crate::config::{Clock, parse_rgba_hex};

pub struct ClockView {
    config: Clock,
    time_text: String,
    date_text: String,
    timer: Option<Task<()>>,
}

impl ClockView {
    pub fn new(cx: &mut Context<Self>, config: &Clock) -> Self {
        let now = Local::now();
        let mut this = Self {
            config: config.clone(),
            time_text: String::new(),
            date_text: String::new(),
            timer: None,
        };
        this.refresh(now);
        this.start_timer(cx);
        this
    }

    pub fn set_config(&mut self, config: &Clock, cx: &mut Context<Self>) {
        let restart = needs_seconds(&self.config.time_format) != needs_seconds(&config.time_format);
        self.config = config.clone();
        self.refresh(Local::now());
        if restart {
            self.start_timer(cx);
        }
        cx.notify();
    }

    fn refresh(&mut self, now: chrono::DateTime<Local>) {
        self.time_text = now.format(&self.config.time_format).to_string();
        self.date_text = now.format(&self.config.date_format).to_string();
    }

    fn start_timer(&mut self, cx: &mut Context<Self>) {
        self.timer.take();
        self.timer = Some(cx.spawn(async move |this, cx| {
            loop {
                let delay = match this.read_with(cx, |this, _| this.next_delay()) {
                    Ok(delay) => delay,
                    Err(_) => break,
                };
                Timer::after(delay).await;

                let now = Local::now();
                let updated = this.update(cx, |this, cx| {
                    this.refresh(now);
                    cx.notify();
                });
                if updated.is_err() {
                    break;
                }
            }
        }));
    }

    fn next_delay(&self) -> Duration {
        let now = Local::now();
        if needs_seconds(&self.config.time_format) {
            let elapsed_ms = now.timestamp_subsec_millis() as u64;
            Duration::from_millis((1000 - elapsed_ms).max(1))
        } else {
            let elapsed_ms = now.second() as u64 * 1000 + now.timestamp_subsec_millis() as u64;
            Duration::from_millis((60_000 - elapsed_ms).max(1))
        }
    }
}

fn needs_seconds(format: &str) -> bool {
    ["%S", "%T", "%r"]
        .iter()
        .any(|token| format.contains(token))
}

impl Render for ClockView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let mut time_el = div()
            .text_size(px(self.config.time_font_size as f32))
            .child(self.time_text.clone());
        let mut date_el = div()
            .text_size(px(self.config.date_font_size as f32))
            .child(self.date_text.clone());

        if self.config.bold {
            time_el = time_el.font_weight(FontWeight::BOLD);
            date_el = date_el.font_weight(FontWeight::BOLD);
        }
        if self.config.italic {
            time_el = time_el.italic();
            date_el = date_el.italic();
        }
        if self.config.underline {
            time_el = time_el.underline();
            date_el = date_el.underline();
        }
        if !self.config.font_family.trim().is_empty() {
            let fam = SharedString::from(self.config.font_family.trim().to_string());
            time_el = time_el.font_family(fam.clone());
            date_el = date_el.font_family(fam);
        }
        if let Some(rgba_val) = self.config.font_color.as_deref().and_then(parse_rgba_hex) {
            let color = rgba(u32::from_be_bytes(rgba_val));
            time_el = time_el.text_color(color);
            date_el = date_el.text_color(color);
        }
        match self.config.alignment {
            crate::config::ClockAlignment::Left => {
                time_el = time_el.text_left();
                date_el = date_el.text_left();
            }
            crate::config::ClockAlignment::Center => {
                time_el = time_el.text_center();
                date_el = date_el.text_center();
            }
            crate::config::ClockAlignment::Right => {
                time_el = time_el.text_right();
                date_el = date_el.text_right();
            }
        }

        let half_gap = self.config.line_gap as f32 / 2.0;
        if self.config.line_gap != 0 {
            time_el = time_el.relative().top(px(-half_gap));
            date_el = date_el.relative().top(px(half_gap));
        }

        let mut root = div().flex().flex_col();
        root = match self.config.alignment {
            crate::config::ClockAlignment::Left => root.items_start(),
            crate::config::ClockAlignment::Center => root.items_center(),
            crate::config::ClockAlignment::Right => root.items_end(),
        };
        root = match self.config.vertical_align {
            crate::config::VerticalAlign::Center => root.justify_center(),
            crate::config::VerticalAlign::Top => root.justify_start(),
            crate::config::VerticalAlign::Bottom => root.justify_end(),
        };

        root.child(time_el).child(date_el)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_second_precision_formats() {
        assert!(!needs_seconds("%H:%M"));
        assert!(needs_seconds("%H:%M:%S"));
        assert!(needs_seconds("%T"));
        assert!(needs_seconds("%r"));
    }
}
