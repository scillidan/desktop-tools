#[cfg(windows)]
use anyhow::{Context, Result};
#[cfg(windows)]
use serde::{Deserialize, Serialize};
#[cfg(windows)]
use std::time::Duration;
#[cfg(windows)]
use taskbar_core::drag::{drag_move, first_move, taskbar_groups};
#[cfg(windows)]
use taskbar_core::pin::pin_order;

#[cfg(windows)]
const APP: &str = "taskbar-pin";

pub const FILE_NAME: &str = "taskbar-pin.toml";
pub const SECTION: &str = "taskbar-pin";
const CONFIG_HELP: &str = "# Move matching taskbar buttons to the front; other buttons keep their order.\n# Entries match an app ID, title, executable name or stem, case-insensitively.\n# Run `taskbar-tools.exe --dump` to list candidates. Empty pin = no explicit order.";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub enabled: bool,
    pub poll_ms: u32,
    pub pin: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            poll_ms: 750,
            pin: Vec::new(),
        }
    }
}

pub fn load() -> Result<Config> {
    taskbar_core::config::load(FILE_NAME, SECTION, None, CONFIG_HELP)
}

pub fn materialize() -> Result<std::path::PathBuf> {
    taskbar_core::config::materialize::<Config>(FILE_NAME, SECTION, None, CONFIG_HELP)
}

#[cfg(windows)]
#[derive(Serialize)]
struct Dump {
    order: Vec<String>,
}

#[cfg(windows)]
pub fn run(shutting_down: impl Fn() -> bool, pins_paused: impl Fn() -> bool) -> Result<()> {
    let automation = taskbar_core::shell::Automation::new()?;
    let mut cfg = load()?;
    while !shutting_down() {
        let result = (|| -> Result<()> {
            cfg = load()?;
            if cfg.enabled && !pins_paused() {
                tick(&automation, &cfg)?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            taskbar_core::win::log(APP, &format!("{error:#}"));
        }
        taskbar_core::win::wait_ms(u64::from(cfg.poll_ms.max(100)), &shutting_down);
    }
    Ok(())
}

#[cfg(windows)]
pub fn once(automation: &taskbar_core::shell::Automation) -> Result<()> {
    let cfg = load()?;
    for _ in 0..100 {
        if !tick(automation, &cfg)? {
            break;
        }
        std::thread::sleep(Duration::from_millis(400));
    }
    Ok(())
}

#[cfg(windows)]
pub fn dump(automation: &taskbar_core::shell::Automation) -> Result<()> {
    let dump = Dump {
        order: taskbar_groups(automation)?
            .into_iter()
            .flat_map(|group| group.ids())
            .collect(),
    };
    print!("{}", toml::to_string_pretty(&dump)?);
    Ok(())
}

#[cfg(windows)]
pub fn inspect(automation: &taskbar_core::shell::Automation) -> Result<()> {
    for bar in taskbar_core::shell::taskbars()? {
        println!(
            "taskbar {:?} secondary={} rect={}",
            bar.hwnd, bar.secondary, bar.rect
        );
        for item in automation.descendants_of(bar.hwnd)? {
            println!(
                "  name={:?} id={:?} class={:?} type={} offscreen={} rect={}",
                item.name,
                item.automation_id,
                item.class_name,
                item.control_type,
                item.offscreen,
                item.rect
            );
        }
    }
    Ok(())
}

/// Check every taskbar and perform at most one physical move per group.
#[cfg(windows)]
fn tick(automation: &taskbar_core::shell::Automation, cfg: &Config) -> Result<bool> {
    let mut moved = false;
    for group in taskbar_groups(automation)? {
        if group.items.is_empty() || cfg.pin.is_empty() {
            continue;
        }
        let target = pin_order(&group.pin_items(), &cfg.pin);
        if let Some((id, index)) = first_move(&group.ids(), &target) {
            drag_move(&group, &id, index)
                .with_context(|| format!("moving {id} in {}", group.key))?;
            moved = true;
        }
    }
    Ok(moved)
}
