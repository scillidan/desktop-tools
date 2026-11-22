#[cfg(any(windows, test))]
mod geometry;
#[cfg(windows)]
mod platform;

#[cfg(windows)]
const APP: &str = "hidden-taskbar";

pub const FILE_NAME: &str = "taskbar-hidden.toml";
pub const SECTION: &str = "hidden-taskbar";
const CONFIG_HELP: &str = "# Auto-hide the taskbar; enabled = false disables the behavior.\n# hotkey toggles visibility. resize_snapped restores snapped windows to the work area.";

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Config {
    pub enabled: bool,
    pub hotkey: String,
    pub resize_snapped: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            hotkey: "ctrl+alt+f9".into(),
            resize_snapped: true,
        }
    }
}

pub fn load() -> anyhow::Result<Config> {
    taskbar_core::config::load(FILE_NAME, SECTION, Some("general"), CONFIG_HELP)
}

pub fn materialize() -> anyhow::Result<std::path::PathBuf> {
    taskbar_core::config::materialize::<Config>(FILE_NAME, SECTION, Some("general"), CONFIG_HELP)
}
#[cfg(windows)]
static THREAD_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

#[cfg(windows)]
pub fn run(
    shutting_down: impl Fn() -> bool,
    hotkeys_suspended: impl Fn() -> bool,
    report: impl Fn(&str, &str),
) -> anyhow::Result<()> {
    use kit::hotkey::Hotkeys;
    use kit::keys::Combo;
    use taskbar_core::message::Event;
    use windows::Win32::UI::WindowsAndMessaging::{KillTimer, PostQuitMessage, SetTimer};

    while !shutting_down() {
        let cfg = load()?;
        let enabled = cfg.enabled;
        let combo: Combo = cfg.hotkey.parse()?;
        let resize_snapped = cfg.resize_snapped;
        if !enabled {
            taskbar_core::win::wait_ms(500, &shutting_down);
            continue;
        }
        THREAD_ID.store(
            unsafe { windows::Win32::System::Threading::GetCurrentThreadId() },
            std::sync::atomic::Ordering::Relaxed,
        );
        let hotkeys = Hotkeys::register(&[("toggle", combo)])?;
        let mut app = platform::App::new(resize_snapped);
        let timer = unsafe { SetTimer(None, 1, 500, None) };
        if timer == 0 {
            anyhow::bail!("SetTimer failed");
        }
        taskbar_core::message::run(Some(&hotkeys), |event| match event {
            Event::Wakeup(usize::MAX) => unsafe { PostQuitMessage(0) },
            Event::Hotkey(label) if label == "toggle" && !hotkeys_suspended() => {
                report_result(app.toggle_foreground(), &report);
            }
            Event::Timer(1) => {
                let result = (|| -> anyhow::Result<()> {
                    let cfg = load()?;
                    let new_combo: Combo = cfg.hotkey.parse()?;
                    if !cfg.enabled || new_combo != combo || cfg.resize_snapped != resize_snapped {
                        unsafe { PostQuitMessage(0) };
                    }
                    Ok(())
                })();
                if let Err(error) = result {
                    taskbar_core::win::log(APP, &format!("{error:#}"));
                }
            }
            _ => {}
        });
        unsafe {
            let _ = KillTimer(None, timer);
        }
        report_warnings(app.restore_all(), &report);
        THREAD_ID.store(0, std::sync::atomic::Ordering::Relaxed);
    }
    Ok(())
}

#[cfg(windows)]
pub fn wake_for_shutdown() {
    let thread = THREAD_ID.load(std::sync::atomic::Ordering::Relaxed);
    if thread != 0 {
        let _ = taskbar_core::win::post_thread_message(thread, usize::MAX, 0);
    }
}

#[cfg(windows)]
fn report_result(result: anyhow::Result<Vec<String>>, report: &dyn Fn(&str, &str)) {
    match result {
        Ok(warnings) => report_warnings(warnings, report),
        Err(e) => report(APP, &format!("Operation failed:\n{e:#}")),
    }
}

#[cfg(windows)]
fn report_warnings(warnings: Vec<String>, report: &dyn Fn(&str, &str)) {
    if !warnings.is_empty() {
        report(
            APP,
            &format!(
                "Some changes could not be applied:\n{}",
                warnings.join("\n")
            ),
        );
    }
}
