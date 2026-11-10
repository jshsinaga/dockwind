use std::time::Duration;

use gpui::{Bounds, Pixels, Point};

#[derive(Debug, Clone, Copy)]
pub struct AutoHideSettings {
    pub show_delay: Duration,
    pub hide_delay: Duration,
    pub animation: Duration,
}

#[derive(Debug, Clone)]
pub struct AutoHide {
    progress: f32,
    target: f32,
    pending_since: Option<Duration>,
    last_update: Duration,
}

impl AutoHide {
    pub fn new(enabled: bool) -> Self {
        let progress = if enabled { 0.0 } else { 1.0 };
        Self {
            progress,
            target: progress,
            pending_since: None,
            last_update: Duration::ZERO,
        }
    }

    pub fn progress(&self) -> f32 {
        self.progress
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        now: Duration,
        cursor: Point<Pixels>,
        dock: Bounds<Pixels>,
        trigger: Bounds<Pixels>,
        settings: AutoHideSettings,
        enabled: bool,
        force_hidden: bool,
        pinned_open: bool,
    ) -> f32 {
        let delta = now.saturating_sub(self.last_update);
        self.last_update = now;

        if !enabled {
            self.progress = 1.0;
            self.target = 1.0;
            self.pending_since = None;
            return self.progress;
        }

        if force_hidden {
            self.progress = 0.0;
            self.target = 0.0;
            self.pending_since = None;
            return self.progress;
        }

        if pinned_open {
            self.target = 1.0;
            self.pending_since = None;
        } else if self.target < 1.0 {
            if contains(trigger, cursor) {
                let since = *self.pending_since.get_or_insert(now);
                if elapsed(now, since) >= settings.show_delay {
                    self.target = 1.0;
                    self.pending_since = None;
                }
            } else {
                self.pending_since = None;
            }
        } else {
            let inside = contains(dock, cursor) || contains(trigger, cursor);
            if inside {
                self.pending_since = None;
            } else {
                let since = *self.pending_since.get_or_insert(now);
                if elapsed(now, since) >= settings.hide_delay {
                    self.target = 0.0;
                    self.pending_since = None;
                }
            }
        }

        let step = if settings.animation.is_zero() {
            1.0
        } else {
            (delta.as_secs_f32() / settings.animation.as_secs_f32()).clamp(0.0, 1.0)
        };

        if self.progress < self.target {
            self.progress = (self.progress + step).min(self.target);
        } else if self.progress > self.target {
            self.progress = (self.progress - step).max(self.target);
        }

        self.progress
    }
}

fn elapsed(now: Duration, since: Duration) -> Duration {
    now.saturating_sub(since)
}

fn contains(bounds: Bounds<Pixels>, point: Point<Pixels>) -> bool {
    point.x >= bounds.origin.x
        && point.x < bounds.origin.x + bounds.size.width
        && point.y >= bounds.origin.y
        && point.y < bounds.origin.y + bounds.size.height
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, px, size};

    fn settings() -> AutoHideSettings {
        AutoHideSettings {
            show_delay: Duration::from_millis(80),
            hide_delay: Duration::from_millis(400),
            animation: Duration::from_millis(180),
        }
    }

    fn dock() -> Bounds<Pixels> {
        Bounds::new(point(px(680.0), px(936.0)), size(px(560.0), px(92.0)))
    }

    fn trigger() -> Bounds<Pixels> {
        Bounds::new(point(px(0.0), px(1038.0)), size(px(1920.0), px(2.0)))
    }

    fn pointer_trigger() -> Point<Pixels> {
        point(px(960.0), px(1039.0))
    }

    fn pointer_outside() -> Point<Pixels> {
        point(px(960.0), px(500.0))
    }

    fn update(auto_hide: &mut AutoHide, now_ms: u64, cursor: Point<Pixels>) -> f32 {
        auto_hide.update(
            Duration::from_millis(now_ms),
            cursor,
            dock(),
            trigger(),
            settings(),
            true,
            false,
            false,
        )
    }

    #[test]
    fn starts_hidden_and_shows_after_delay_then_animation() {
        let mut auto_hide = AutoHide::new(true);
        assert_eq!(update(&mut auto_hide, 0, pointer_trigger()), 0.0);
        assert_eq!(update(&mut auto_hide, 50, pointer_trigger()), 0.0);
        assert!(update(&mut auto_hide, 90, pointer_trigger()) > 0.0);
        assert_eq!(update(&mut auto_hide, 400, pointer_trigger()), 1.0);
    }

    #[test]
    fn hides_after_cursor_leaves_and_delay() {
        let mut auto_hide = AutoHide::new(true);
        update(&mut auto_hide, 0, pointer_trigger());
        assert_eq!(update(&mut auto_hide, 400, pointer_trigger()), 1.0);
        assert_eq!(update(&mut auto_hide, 500, pointer_outside()), 1.0);
        assert!(update(&mut auto_hide, 950, pointer_outside()) < 1.0);
        assert_eq!(update(&mut auto_hide, 1200, pointer_outside()), 0.0);
    }

    #[test]
    fn quick_pass_does_not_flicker() {
        let mut auto_hide = AutoHide::new(true);
        update(&mut auto_hide, 0, pointer_trigger());
        update(&mut auto_hide, 50, pointer_trigger());
        assert_eq!(update(&mut auto_hide, 70, pointer_outside()), 0.0);
    }

    #[test]
    fn disabled_and_forced_states_short_circuit() {
        let mut auto_hide = AutoHide::new(true);
        auto_hide.update(
            Duration::from_millis(100),
            pointer_outside(),
            dock(),
            trigger(),
            settings(),
            false,
            false,
            false,
        );
        assert_eq!(auto_hide.progress(), 1.0);

        let mut auto_hide = AutoHide::new(true);
        update(&mut auto_hide, 0, pointer_trigger());
        auto_hide.update(
            Duration::from_millis(500),
            pointer_trigger(),
            dock(),
            trigger(),
            settings(),
            true,
            true,
            false,
        );
        assert_eq!(auto_hide.progress(), 0.0);
    }
}
