//! Explorer taskbar primitives shared by the native taskbar features.

pub mod pin;

#[cfg(windows)]
pub mod config;
#[cfg(windows)]
pub mod drag;
#[cfg(windows)]
pub mod message;
#[cfg(windows)]
pub mod shell;
#[cfg(windows)]
pub mod win;
