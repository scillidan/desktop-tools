//! Rectangle-style window geometry and snapping.

pub mod geometry;
#[cfg(windows)]
pub mod mouse;
mod state;

pub use geometry::{Rect, SizeSpec};
pub use state::{Rectangle, RectangleAction, RectanglePlatform, SizerPick};
