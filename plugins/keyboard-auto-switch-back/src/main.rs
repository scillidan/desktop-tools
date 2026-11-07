#![cfg_attr(windows, windows_subsystem = "windows")]

//! keyboard-auto-switch-back: when the foreground window changes, wait
//! `switch_delay` ms and then force the target keyboard layout via
//! `WM_INPUTLANGCHANGEREQUEST`.
//! Ported from AHK-KeyboardAutoSwitchBack.
//!
//! Parity notes:
//! - The AHK original polls every 200 ms (`CheckInterval`); we use
//!   `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)`, which is event-driven and
//!   needs no polling. `CheckInterval` is therefore dropped.
//! - The AHK one-shot timer is a debounce: each foreground change restarts the
//!   delay, and only the latest window is switched. The worker below does the
//!   same with `recv_timeout`: an event that arrives during the delay replaces
//!   the pending one and restarts the wait, so a quick Alt-Tab cannot apply a
//!   stale window.
//! - TargetLayout is an HKL (`0x04090409`); only its low word (language ID) is
//!   compared, exactly like the AHK `targetLayout & 0xFFFF`.

fn main() -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        if let Err(e) = run() {
            kit::win::fatal_error("keyboard-auto-switch-back", &format!("Error:\n{e:#}"));
        }
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        kit::unimplemented("keyboard-auto-switch-back")
    }
}

#[cfg(windows)]
use std::sync::atomic::{AtomicIsize, AtomicU64, Ordering};
#[cfg(windows)]
use std::sync::{mpsc, OnceLock};

/// Opt-in trace: `KBTOOLS_DEBUG=1` appends to
/// `%TEMP%\keyboard-auto-switch-back.log`.
#[cfg(windows)]
fn debug_log(msg: impl AsRef<str>) {
    if std::env::var_os("KBTOOLS_DEBUG").is_none() {
        return;
    }
    use std::io::Write;
    let path = std::env::temp_dir().join("keyboard-auto-switch-back.log");
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "{}", msg.as_ref());
    }
}

#[cfg(windows)]
static LATEST: Latest = Latest::new();
/// Skip repeated `EVENT_SYSTEM_FOREGROUND` notifications for the same window.
#[cfg(windows)]
static LAST_HWND: AtomicIsize = AtomicIsize::new(0);
/// The WinEvent callback is a bare fn; a global sender avoids relying on a
/// thread-local being visible in the callback.
#[cfg(windows)]
static TX: OnceLock<mpsc::Sender<(u64, isize)>> = OnceLock::new();

/// Monotonic event number. Bumped in the hook callback (at event time), not
/// when the worker picks the event up; the worker applies only if its number
/// is still the latest after the debounce delay.
#[cfg(windows)]
struct Latest(AtomicU64);

#[cfg(windows)]
impl Latest {
    const fn new() -> Self {
        Self(AtomicU64::new(0))
    }

    fn bump(&self) -> u64 {
        self.0.fetch_add(1, Ordering::SeqCst) + 1
    }

    fn is_current(&self, seq: u64) -> bool {
        self.0.load(Ordering::SeqCst) == seq
    }
}

/// Parse `0x04090409`, `0X04090409` or bare `04090409` as an HKL.
#[cfg(windows)]
fn parse_hkl(raw: &str) -> anyhow::Result<isize> {
    use anyhow::Context;
    let raw = raw.trim();
    let hex = raw
        .strip_prefix("0x")
        .or_else(|| raw.strip_prefix("0X"))
        .unwrap_or(raw);
    isize::from_str_radix(hex, 16)
        .with_context(|| format!("target_layout must be a hex HKL like 0x04090409, got `{raw}`"))
}

/// Compare the language ID (low word), not the full HKL.
#[cfg(windows)]
fn needs_switch(current_hkl: u32, target_lang_id: u32) -> bool {
    (current_hkl & 0xFFFF) != (target_lang_id & 0xFFFF)
}

#[cfg(windows)]
fn run() -> anyhow::Result<()> {
    use kit::tray::{Event, Tray};
    use kit::{config, win};
    use serde::Deserialize;
    use std::thread;
    use std::time::Duration;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
    use windows::Win32::UI::WindowsAndMessaging::{EVENT_SYSTEM_FOREGROUND, WINEVENT_OUTOFCONTEXT};

    const APP: &str = "keyboard-auto-switch-back";
    let hosted = std::env::args().any(|a| a == "--plugin");
    let no_tray = hosted && std::env::args().any(|a| a == "--no-tray");

    let _single = win::ensure_single_instance(APP)?;

    #[derive(Deserialize, Default)]
    struct General {
        switch_delay: Option<u64>,
        target_layout: Option<String>,
    }
    #[derive(Deserialize, Default)]
    struct FileConfig {
        general: Option<General>,
    }

    let (config_path, text) = config::load_profile(
        "keyboard-auto-switch-back.toml",
        include_str!("../keyboard-auto-switch-back.toml"),
    )?;
    let file: FileConfig = toml::from_str(&text)?;
    let general = file.general.unwrap_or_default();
    let switch_delay = general.switch_delay.unwrap_or(600);
    let target_layout = parse_hkl(general.target_layout.as_deref().unwrap_or("0x04090409"))?;
    let target_lang_id = (target_layout as u32) & 0xFFFF;

    let layout_name =
        win::layout_text(target_lang_id).unwrap_or_else(|| format!("0x{target_lang_id:08X}"));
    let tooltip = format!("{APP}\nTarget layout: {layout_name}");
    debug_log(format!(
        "start delay={switch_delay} target=0x{target_layout:08x} profile={}",
        config_path.display()
    ));

    // Foreground events -> debounced worker. HWND is not Send, so shuttle it
    // as isize. The worker folds bursts into the last event.
    let (tx, rx) = mpsc::channel::<(u64, isize)>();
    TX.set(tx).ok();
    thread::spawn(move || {
        let delay = Duration::from_millis(switch_delay);
        let mut pending: Option<(u64, isize)> = None;
        loop {
            let (seq, raw_hwnd) = match pending.take() {
                Some(event) => event,
                None => match rx.recv() {
                    Ok(event) => event,
                    Err(_) => break,
                },
            };
            match rx.recv_timeout(delay) {
                // A newer foreground change arrived during the delay: it
                // replaces this one and restarts the wait.
                Ok(next) => {
                    debug_log(format!("reset seq={} -> {}", seq, next.0));
                    pending = Some(next);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if LATEST.is_current(seq) {
                        debug_log(format!("apply seq={seq} hwnd={raw_hwnd:x}"));
                        let hwnd = HWND(raw_hwnd as *mut core::ffi::c_void);
                        apply_layout(hwnd, target_layout, target_lang_id);
                    } else {
                        debug_log(format!("skip stale seq={seq}"));
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    });

    unsafe extern "system" fn foreground_hook(
        _hook: HWINEVENTHOOK,
        _event: u32,
        hwnd: HWND,
        _id_object: i32,
        _id_child: i32,
        _event_thread: u32,
        _event_time: u32,
    ) {
        if hwnd.0.is_null() {
            return;
        }
        let raw = hwnd.0 as isize;
        if LAST_HWND.swap(raw, Ordering::SeqCst) == raw {
            return; // same top-level window, just a focus change
        }
        let seq = LATEST.bump();
        debug_log(format!("foreground seq={seq} hwnd={raw:x}"));
        if let Some(tx) = TX.get() {
            let _ = tx.send((seq, raw));
        }
    }
    let hook: HWINEVENTHOOK = unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(foreground_hook),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        )
    };
    if hook.0.is_null() {
        anyhow::bail!("SetWinEventHook(EVENT_SYSTEM_FOREGROUND) failed");
    }
    let _unhook = UnhookGuard(hook);

    if no_tray {
        use kit::ipc::HostCommand;
        let link = kit::ipc::PluginLink::new()?;
        link.ready(kit::menu::MenuModel::default(), Some("running".into()))?;
        kit::tray::run_headless(None, |event| {
            if let Event::Wakeup(4) = event {
                while let Some(command) = link.try_recv() {
                    if matches!(command, HostCommand::Quit { .. }) {
                        kit::tray::quit();
                    }
                }
            }
        });
        return Ok(());
    }

    let mut tray = Tray::new(
        &tooltip,
        kit::icon::letter_rgba('k', [0x3b, 0x6e, 0xa5, 0xff], [0x23, 0x46, 0x6e, 0xff]),
    )?;
    refresh_menu(&mut tray, hosted);

    tray.run(None, |tray, event| {
        if let Event::Menu(label) = event {
            if label == "Start with Windows" {
                let now = !win::autostart_enabled(APP);
                if let Err(e) = win::set_autostart(APP, now) {
                    win::message_box(APP, &format!("Autostart failed:\n{e:#}"));
                }
                refresh_menu(tray, hosted);
            } else if label == "Edit Config" {
                win::edit_config(&config_path);
            } else if label == "Reload" {
                config::restart_self();
                tray.quit();
            } else if label == "Exit" {
                tray.quit();
            }
        }
    });
    Ok(())
}

#[cfg(windows)]
fn apply_layout(hwnd: windows::Win32::Foundation::HWND, target_layout: isize, target_lang_id: u32) {
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, PostMessageW};
    const WM_INPUTLANGCHANGEREQUEST: u32 = 0x50;
    unsafe {
        if GetForegroundWindow() != hwnd {
            debug_log("apply skipped: foreground changed");
            return; // window is not foreground anymore (AHK: curID != lastWinID)
        }
        let Some(cur) = kit::win::layout_of_window(hwnd) else {
            debug_log("apply skipped: no window layout");
            return;
        };
        if needs_switch(cur as u32, target_lang_id) {
            debug_log(format!(
                "post hkl=0x{target_layout:08x} current=0x{:08x}",
                (cur as u32) & 0xFFFF
            ));
            let _ = PostMessageW(
                Some(hwnd),
                WM_INPUTLANGCHANGEREQUEST,
                windows::Win32::Foundation::WPARAM(0),
                windows::Win32::Foundation::LPARAM(target_layout),
            );
        } else {
            debug_log("already at target");
        }
    }
}

#[cfg(windows)]
struct UnhookGuard(windows::Win32::UI::Accessibility::HWINEVENTHOOK);

#[cfg(windows)]
impl Drop for UnhookGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::UI::Accessibility::UnhookWinEvent(self.0);
        }
    }
}

#[cfg(windows)]
fn refresh_menu(tray: &mut kit::tray::Tray, hosted: bool) {
    use kit::tray::Entry;
    let mut entries = Vec::new();
    if !hosted {
        entries.push(Entry::Toggle(
            "Start with Windows",
            kit::win::autostart_enabled("keyboard-auto-switch-back"),
        ));
        entries.push(Entry::Sep);
    }
    entries.push(Entry::Item("Edit Config"));
    if !hosted {
        entries.push(Entry::Item("Reload"));
        entries.push(Entry::Item("Exit"));
    }
    tray.set_menu(&entries);
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn hkl_accepts_both_prefixes_and_bare_hex() {
        assert_eq!(parse_hkl("0x04090409").unwrap(), 0x04090409);
        assert_eq!(parse_hkl("0X04090409").unwrap(), 0x04090409);
        assert_eq!(parse_hkl("04090409").unwrap(), 0x04090409);
        assert_eq!(parse_hkl(" 0x0804 ").unwrap(), 0x0804);
        assert!(parse_hkl("not-hex").is_err());
        assert!(parse_hkl("0x").is_err());
    }

    #[test]
    fn low_word_is_the_language_id() {
        assert!(!needs_switch(0x04090409, 0x0409));
        assert!(needs_switch(0x08040804, 0x0409));
        assert!(!needs_switch(0x08040409, 0x0409));
    }

    #[test]
    fn only_the_latest_sequence_may_apply() {
        let latest = Latest::new();
        let first = latest.bump();
        assert!(latest.is_current(first));
        let second = latest.bump();
        assert!(!latest.is_current(first));
        assert!(latest.is_current(second));
    }
}
