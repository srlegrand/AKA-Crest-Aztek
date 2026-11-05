//! Aztek's foundation: paths, the workspace, layered prefs and the main-thread queue. Ported from `src/core`.

pub mod main_queue;
pub mod paths;
pub mod prefs;
pub mod workspace;

pub use prefs::{PrefScope, PrefStore};
pub use workspace::Workspace;
