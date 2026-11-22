//! Windows tray icon + menu and HKCU Run autostart.
//!
//! The icon lives on the daemon's main thread: its hidden window must be
//! pumped by the same `GetMessageW` loop that dispatches `WM_HOTKEY`.
//! Menu clicks arrive through `MenuEvent`'s global channel, so the loop only
//! needs a non-blocking `try_recv` after each message (keytools-rs pattern).

use anyhow::{Context, Result};
use kit::tray::{Event, Tray as KitTray};
use windows::core::HSTRING;
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    RegDeleteValueW, RegGetValueW, RegOpenKeyExW, RegSetValueExW, HKEY_CURRENT_USER, KEY_SET_VALUE,
    REG_SZ, RRF_RT_REG_SZ,
};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const APP_NAME: &str = "window-tools";
/// Older autostart value names, migrated once by `migrate_autostart`.
const OLD_APP_NAMES: &[&str] = &[
    "desktop-tools-rs",
    "window-tools-rs",
    "windowtools-rs",
    "rectangle-rs",
];

pub use crate::menu::Cmd;

/// What the icon shows. `Reload*` are transient; the daemon reverts them.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum State {
    Normal,
    Suspended,
    ReloadOk,
    ReloadFail,
}

pub struct Tray {
    icon: KitTray,
    autostart: bool,
    suspended: bool,
    state: State,
}

impl Tray {
    pub fn new() -> Result<Tray> {
        migrate_autostart();
        let autostart = autostart_enabled();
        let icon = KitTray::new(APP_NAME, icon(State::Normal, false))?;
        let mut tray = Tray {
            icon,
            autostart,
            suspended: false,
            state: State::Normal,
        };
        tray.rebuild();
        Ok(tray)
    }

    /// Tooltip text; truncated to the 128-char NOTIFYICONDATA limit.
    pub fn set_status(&mut self, text: &str) {
        let text: String = text.chars().take(120).collect();
        self.icon.set_tooltip(&text);
    }

    /// Swap the icon. A failed swap only loses the visual cue, so it is not
    /// reported to the caller.
    pub fn set_state(&mut self, state: State) {
        if state == self.state {
            return;
        }
        self.state = state;
        let _ = self.icon.set_icon(icon(state, self.suspended));
    }

    pub fn set_suspended(&mut self, suspended: bool) {
        self.suspended = suspended;
        self.rebuild();
        self.set_state(if suspended {
            State::Suspended
        } else {
            State::Normal
        });
    }

    pub fn toggle_autostart(&mut self) -> Result<()> {
        let on = !self.autostart;
        set_autostart(on)?;
        self.autostart = on;
        self.rebuild();
        Ok(())
    }

    /// Next menu click, if any. Never blocks, so hotkeys stay responsive.
    pub fn poll(&self) -> Option<Cmd> {
        match self.icon.poll()? {
            Event::MenuId(id) => crate::menu::command(&id),
            Event::Menu(label) => crate::menu::model(self.suspended, self.autostart, true)
                .id_for_label(&label)
                .and_then(crate::menu::command),
            _ => None,
        }
    }

    fn rebuild(&mut self) {
        let model = crate::menu::model(self.suspended, self.autostart, true);
        self.icon.set_menu(&model.render());
    }
}

fn icon(state: State, suspended: bool) -> Vec<u8> {
    use kit::icon::State as Icon;
    let state = match (state, suspended) {
        (State::ReloadOk, true) => Icon::SuspendedSuccess,
        (State::ReloadFail, true) => Icon::SuspendedError,
        (State::ReloadOk, false) => Icon::Success,
        (State::ReloadFail, false) => Icon::Error,
        (State::Suspended, _) | (_, true) => Icon::Suspended,
        (State::Normal, false) => Icon::Normal,
    };
    kit::icon::rgba(kit::icon::WINDOW, state)
}

/// The old builds registered `HKCU\Run\window-tools-rs` / `windowtools-rs` /
/// `rectangle-rs`; move that setting to `desktop-tools-rs` with the current
/// exe path so autostart survives the rename.
fn migrate_autostart() {
    for old in OLD_APP_NAMES {
        if run_value(old).is_some() {
            if run_value(APP_NAME).is_none() {
                if let Err(e) = set_autostart(true) {
                    eprintln!("warn: cannot migrate autostart: {e:#}");
                }
            }
            delete_run_value(old);
        }
    }
    // Moving the folder leaves the Run value pointing at the old exe path.
    if autostart_enabled() {
        if let Ok(exe) = std::env::current_exe() {
            let want = format!("\"{}\"", exe.display());
            if run_value(APP_NAME).as_deref() != Some(want.as_str()) {
                if let Err(e) = set_autostart(true) {
                    eprintln!("warn: cannot refresh autostart path: {e:#}");
                }
            }
        }
    }
}

fn run_value(name: &str) -> Option<String> {
    let subkey = HSTRING::from(RUN_KEY);
    let value = HSTRING::from(name);
    let mut len = 0u32;
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &subkey,
            &value,
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut len),
        )
    };
    if r != ERROR_SUCCESS || len == 0 {
        return None;
    }
    let mut buf = vec![0u16; (len as usize).div_ceil(2)];
    let mut bytes = (buf.len() * 2) as u32;
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &subkey,
            &value,
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr() as *mut _),
            Some(&mut bytes),
        )
    };
    if r != ERROR_SUCCESS {
        return None;
    }
    Some(
        String::from_utf16_lossy(&buf)
            .trim_end_matches('\0')
            .to_string(),
    )
}

fn delete_run_value(name: &str) {
    let subkey = HSTRING::from(RUN_KEY);
    let value = HSTRING::from(name);
    let mut key = windows::Win32::System::Registry::HKEY::default();
    let r = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, &subkey, Some(0), KEY_SET_VALUE, &mut key) };
    if r == ERROR_SUCCESS {
        unsafe {
            let _ = RegDeleteValueW(key, &value);
            let _ = windows::Win32::System::Registry::RegCloseKey(key);
        }
    }
}

fn autostart_enabled() -> bool {
    run_value(APP_NAME).is_some()
}

fn set_autostart(on: bool) -> Result<()> {
    let subkey = HSTRING::from(RUN_KEY);
    let value = HSTRING::from(APP_NAME);
    let mut key = windows::Win32::System::Registry::HKEY::default();
    let r = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, &subkey, Some(0), KEY_SET_VALUE, &mut key) };
    anyhow::ensure!(
        r == ERROR_SUCCESS,
        "cannot open HKCU\\{RUN_KEY} (error {})",
        r.0
    );
    let result = unsafe {
        if on {
            // Quoted so a path with spaces survives Run's command-line parse.
            let exe = std::env::current_exe().context("cannot locate own executable")?;
            let cmd = format!("\"{}\"", exe.display());
            let wide: Vec<u16> = cmd.encode_utf16().chain(std::iter::once(0)).collect();
            let bytes = std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2);
            RegSetValueExW(key, &value, Some(0), REG_SZ, Some(bytes))
        } else {
            RegDeleteValueW(key, &value)
        }
    };
    let _ = unsafe { windows::Win32::System::Registry::RegCloseKey(key) };
    anyhow::ensure!(
        result == ERROR_SUCCESS,
        "cannot {} the autostart value (error {})",
        if on { "write" } else { "remove" },
        result.0
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_state_draws_a_distinct_icon() {
        let normal = icon(State::Normal, false);
        assert_ne!(normal, icon(State::Suspended, true));
        assert_ne!(normal, icon(State::ReloadOk, false));
        assert_ne!(normal, icon(State::ReloadFail, false));
        assert_ne!(icon(State::ReloadOk, false), icon(State::ReloadFail, false));
        assert_ne!(icon(State::ReloadOk, true), icon(State::ReloadOk, false));
    }

    #[test]
    fn embedded_asset_matches_the_shared_renderer() {
        assert_eq!(
            include_bytes!("../assets/window-tools.ico").as_slice(),
            kit::icon::ico(kit::icon::WINDOW, kit::icon::State::Normal)
        );
    }
}
