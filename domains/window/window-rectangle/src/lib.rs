//! Rectangle-style window geometry and snapping.

pub mod geometry;
#[cfg(windows)]
mod state;

pub use geometry::{Rect, SizeSpec};
pub use state::{Rectangle, RectangleAction};
pub use window_core::{SizerPick, WindowOps as RectanglePlatform};
