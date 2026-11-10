mod model;
mod store;
mod validation;
mod watch;

pub use model::{Appearance, BlurMode, Clock, Config, DockEdge, PinnedApp};
pub use store::default_path;
pub use validation::parse_rgba_hex;
pub use watch::{ConfigWatcher, watch};
