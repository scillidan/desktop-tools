//! Host-side plugin contract: manifest validation, catalog discovery, core
//! config, and restart backoff. No Win32 and no process spawning here; the host
//! owns the lifecycle so this crate stays unit-testable.

pub mod catalog;
pub mod core_config;
pub mod lifecycle;
pub mod manifest;
