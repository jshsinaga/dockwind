mod model;
mod store;
mod validation;
mod watch;

pub use model::{
    AlignmentAnchor, AnimationCurve, Appearance, BlurMode, Clock, ClockAlignment, Config, DockEdge,
    IndicatorStyle, ItemAlignment, PinnedApp, VerticalAlign, WidthMode,
};
pub use store::{ConfigError, default_path};
pub use validation::{parse_rgba_hex, valid_strftime};
pub use watch::{ConfigWatcher, watch};
