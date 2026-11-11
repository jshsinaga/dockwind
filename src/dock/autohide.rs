use std::time::Duration;

#[derive(Debug, Clone, Copy)]
pub struct AutoHideSettings {
    pub show_delay: Duration,
    pub hide_delay: Duration,
    pub animation: Duration,
}

#[derive(Debug, Clone, Copy)]
pub struct AutoHideInput {
    pub now: Duration,
    pub pointer_in_dock: bool,
    pub pointer_in_trigger: bool,
    pub enabled: bool,
    pub force_hidden: bool,
    pub pinned_open: bool,
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

    pub fn is_animating(&self) -> bool {
        (self.progress - self.target).abs() > 0.0001
    }

    pub fn update_target(&mut self, input: AutoHideInput, settings: AutoHideSettings) -> bool {
        let old_target = self.target;
        if !input.enabled {
            self.target = 1.0;
            self.pending_since = None;
            return old_target != self.target;
        }

        if input.force_hidden {
            self.target = 0.0;
            self.pending_since = None;
            return old_target != self.target;
        }

        if input.pinned_open {
            self.target = 1.0;
            self.pending_since = None;
        } else if self.target < 1.0 {
            if input.pointer_in_trigger {
                let since = *self.pending_since.get_or_insert(input.now);
                if elapsed(input.now, since) >= settings.show_delay {
                    self.target = 1.0;
                    self.pending_since = None;
                }
            } else {
                self.pending_since = None;
            }
        } else {
            let inside = input.pointer_in_dock || input.pointer_in_trigger;
            if inside {
                self.pending_since = None;
            } else {
                let since = *self.pending_since.get_or_insert(input.now);
                if elapsed(input.now, since) >= settings.hide_delay {
                    self.target = 0.0;
                    self.pending_since = None;
                }
            }
        }

        old_target != self.target
    }

    pub fn advance(&mut self, now: Duration, settings: AutoHideSettings) -> f32 {
        let delta = now.saturating_sub(self.last_update);
        self.last_update = now;

        if !self.is_animating() {
            return self.progress;
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

    #[cfg(test)]
    pub fn update(&mut self, input: AutoHideInput, settings: AutoHideSettings) -> f32 {
        self.update_target(input, settings);
        if !input.enabled {
            self.progress = 1.0;
            self.target = 1.0;
            self.pending_since = None;
            self.last_update = input.now;
            return self.progress;
        }
        if input.force_hidden {
            self.progress = 0.0;
            self.target = 0.0;
            self.pending_since = None;
            self.last_update = input.now;
            return self.progress;
        }
        self.advance(input.now, settings)
    }
}

fn elapsed(now: Duration, since: Duration) -> Duration {
    now.saturating_sub(since)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> AutoHideSettings {
        AutoHideSettings {
            show_delay: Duration::from_millis(80),
            hide_delay: Duration::from_millis(400),
            animation: Duration::from_millis(180),
        }
    }

    fn input(now_ms: u64, pointer_in_dock: bool, pointer_in_trigger: bool) -> AutoHideInput {
        AutoHideInput {
            now: Duration::from_millis(now_ms),
            pointer_in_dock,
            pointer_in_trigger,
            enabled: true,
            force_hidden: false,
            pinned_open: false,
        }
    }

    #[test]
    fn starts_hidden_and_shows_after_delay_then_animation() {
        let mut auto_hide = AutoHide::new(true);
        assert_eq!(auto_hide.update(input(0, false, true), settings()), 0.0);
        assert_eq!(auto_hide.update(input(50, false, true), settings()), 0.0);
        assert!(auto_hide.update(input(90, false, true), settings()) > 0.0);
        assert_eq!(auto_hide.update(input(400, false, true), settings()), 1.0);
    }

    #[test]
    fn hides_after_pointer_leaves_and_delay() {
        let mut auto_hide = AutoHide::new(true);
        auto_hide.update(input(0, false, true), settings());
        assert_eq!(auto_hide.update(input(400, false, true), settings()), 1.0);
        assert_eq!(auto_hide.update(input(500, false, false), settings()), 1.0);
        assert!(auto_hide.update(input(950, false, false), settings()) < 1.0);
        assert_eq!(auto_hide.update(input(1200, false, false), settings()), 0.0);
    }

    #[test]
    fn quick_pass_does_not_flicker() {
        let mut auto_hide = AutoHide::new(true);
        auto_hide.update(input(0, false, true), settings());
        auto_hide.update(input(50, false, true), settings());
        assert_eq!(auto_hide.update(input(70, false, false), settings()), 0.0);
    }

    #[test]
    fn pinned_open_keeps_the_dock_visible() {
        let mut auto_hide = AutoHide::new(true);
        let mut pinned = input(0, false, false);
        pinned.pinned_open = true;
        assert_eq!(auto_hide.update(pinned, settings()), 0.0);

        pinned.now = Duration::from_millis(400);
        assert_eq!(auto_hide.update(pinned, settings()), 1.0);
    }

    #[test]
    fn disabled_and_forced_states_short_circuit() {
        let mut auto_hide = AutoHide::new(true);
        let mut disabled = input(100, false, false);
        disabled.enabled = false;
        assert_eq!(auto_hide.update(disabled, settings()), 1.0);

        let mut auto_hide = AutoHide::new(true);
        auto_hide.update(input(0, false, true), settings());
        let mut forced = input(500, false, true);
        forced.force_hidden = true;
        assert_eq!(auto_hide.update(forced, settings()), 0.0);
    }
}
