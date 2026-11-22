#[cfg(any(windows, test))]
mod geometry;
#[cfg(windows)]
mod platform;

#[cfg(windows)]
use anyhow::Result;
#[cfg(windows)]
use serde::{Deserialize, Serialize};
#[cfg(windows)]
use std::time::{Duration, Instant};

#[cfg(windows)]
const APP_NAME: &str = "center-taskbar";

pub const FILE_NAME: &str = "taskbar-center.toml";
pub const SECTION: &str = "center-taskbar";
const CONFIG_HELP: &str = "# Center the legacy taskbar list; enabled = false restores Explorer's layout.\n# poll_ms is the geometry polling interval in milliseconds (minimum 10).";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub enabled: bool,
    pub poll_ms: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            poll_ms: 100,
        }
    }
}

impl Config {
    pub fn validate(&self) -> Result<()> {
        if self.poll_ms < 10 {
            anyhow::bail!("center-taskbar poll_ms must be at least 10");
        }
        Ok(())
    }
}

pub fn load() -> Result<Config> {
    taskbar_core::config::load(FILE_NAME, SECTION, None, CONFIG_HELP)
}

pub fn materialize() -> Result<std::path::PathBuf> {
    taskbar_core::config::materialize::<Config>(FILE_NAME, SECTION, None, CONFIG_HELP)
}

#[cfg(windows)]
pub fn run(shutting_down: impl Fn() -> bool, suspended: impl Fn() -> bool) -> Result<()> {
    taskbar_core::win::log(APP_NAME, "starting");
    let mut poll_ms = 100;
    let mut legacy = false;
    let mut legacy_lists = Vec::new();
    let mut native = None;
    let mut native_check = Instant::now();
    platform::with_com(|| {
        taskbar_core::win::log(APP_NAME, "COM initialized");
        while !shutting_down() {
            let result = (|| -> Result<()> {
                let config = load()?;
                config.validate()?;
                poll_ms = config.poll_ms;
                if !centering_active(config.enabled, suspended()) {
                    platform::restore_legacy(&legacy_lists);
                    legacy_lists.clear();
                    legacy = false;
                    native = None;
                    return Ok(());
                }
                if !legacy {
                    if native.is_none() {
                        if platform::discover()?.mode() == platform::Mode::Legacy {
                            legacy = true;
                            taskbar_core::win::log(APP_NAME, "legacy taskbar mode");
                        } else {
                            native = Some(platform::NativeAlignment::enable()?);
                            native_check = Instant::now();
                            taskbar_core::win::log(APP_NAME, "native taskbar alignment mode");
                        }
                    } else if native_check.elapsed() >= Duration::from_secs(1) {
                        native_check = Instant::now();
                        if platform::discover()?.mode() == platform::Mode::Legacy {
                            if let Some(mut alignment) = native.take() {
                                alignment.restore()?;
                            }
                            legacy = true;
                            taskbar_core::win::log(
                                APP_NAME,
                                "legacy task list appeared; switched from native alignment",
                            );
                        }
                    }
                }
                if legacy {
                    legacy_lists = platform::legacy_step()?;
                }
                Ok(())
            })();
            if let Err(error) = result {
                taskbar_core::win::log(APP_NAME, &format!("{error:#}"));
            }
            taskbar_core::win::wait_ms(poll_ms, &shutting_down);
        }
        platform::restore_legacy(&legacy_lists);
        Ok(())
    })
}

fn centering_active(configured: bool, suspended: bool) -> bool {
    configured && !suspended
}

#[cfg(windows)]
pub fn inspect() -> Result<()> {
    let discovery = platform::with_com(platform::discover)?;
    platform::print_discovery(&discovery);
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn group_suspend_pauses_centering_without_changing_configuration() {
        assert!(super::centering_active(true, false));
        assert!(!super::centering_active(true, true));
        assert!(!super::centering_active(false, false));
    }
}
