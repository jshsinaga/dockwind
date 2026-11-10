use std::time::Duration;

use chrono::{Local, Timelike};
use gpui::{Context, Render, Task, Timer, Window, div, prelude::*};

use crate::config::Clock;

pub struct ClockView {
    time_format: String,
    date_format: String,
    time_text: String,
    date_text: String,
    timer: Option<Task<()>>,
}

impl ClockView {
    pub fn new(cx: &mut Context<Self>, config: &Clock) -> Self {
        let now = Local::now();
        let mut this = Self {
            time_format: config.time_format.clone(),
            date_format: config.date_format.clone(),
            time_text: String::new(),
            date_text: String::new(),
            timer: None,
        };
        this.refresh(now);
        this.start_timer(cx);
        this
    }

    pub fn set_config(&mut self, config: &Clock, cx: &mut Context<Self>) {
        let restart = needs_seconds(&self.time_format) != needs_seconds(&config.time_format);
        self.time_format = config.time_format.clone();
        self.date_format = config.date_format.clone();
        self.refresh(Local::now());
        if restart {
            self.start_timer(cx);
        }
        cx.notify();
    }

    fn refresh(&mut self, now: chrono::DateTime<Local>) {
        self.time_text = now.format(&self.time_format).to_string();
        self.date_text = now.format(&self.date_format).to_string();
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
        if needs_seconds(&self.time_format) {
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
        div()
            .flex()
            .flex_col()
            .items_end()
            .child(self.time_text.clone())
            .child(self.date_text.clone())
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
