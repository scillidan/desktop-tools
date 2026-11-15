#![cfg_attr(windows, windows_subsystem = "windows")]

//! keyboard-switch: press the switch hotkey (default CapsLock) to cycle the
//! keyboard layout — through all installed layouts (InAll) or a configured
//! list applied via Alt+Shift+N (InDefine). Ported from AHK-KeyboardSwitch.
//!
//! CapsLock/Esc use a low-level keyboard hook, like the AHK `$` hotkeys:
//! physical keys are swallowed and the action is posted to the main thread.
//! Injected keys (`LLKHF_INJECTED`) pass through, so a synthesized Esc does
//! not re-trigger the hook. Other switch hotkeys use RegisterHotKey.
//!
//! No switch popup: the merged suite has no per-plugin UI. The layout name
//! stays in the tray tooltip until the host replaces the tray.

#[cfg(windows)]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(windows)]
static SWAP_ESC: AtomicBool = AtomicBool::new(false);

/// Opt-in trace: `KBTOOLS_DEBUG=1` appends to
/// `%TEMP%\keyboard-switch.log`.
#[cfg(windows)]
fn debug_log(msg: impl AsRef<str>) {
    if std::env::var_os("KBTOOLS_DEBUG").is_none() {
        return;
    }
    use std::io::Write;
    let path = std::env::temp_dir().join("keyboard-switch.log");
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "{}", msg.as_ref());
    }
}

fn main() -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        if let Err(e) = run() {
            kit::win::fatal_error("keyboard-switch", &format!("Error:\n{e:#}"));
        }
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        kit::unimplemented("keyboard-switch")
    }
}

#[cfg(windows)]
#[derive(Clone, serde::Deserialize)]
struct DefinedLayout {
    klid: String,
    shortcut: Option<String>,
}

#[cfg(windows)]
#[derive(serde::Deserialize)]
struct Settings {
    switch_mode: Option<String>,
    switch_hotkey: Option<String>,
    swap_esc_caps: Option<bool>,
}

#[cfg(windows)]
#[derive(serde::Deserialize)]
struct FileConfig {
    settings: Option<Settings>,
    layout: Option<Vec<DefinedLayout>>,
}

/// Installed layouts, cached like the AHK `GetInstalledLayouts()` static.
#[cfg(windows)]
fn installed_layouts() -> Vec<isize> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyboardLayoutList, HKL};
    let count = unsafe { GetKeyboardLayoutList(None) };
    if count <= 0 {
        return Vec::new();
    }
    let mut buf = vec![HKL(std::ptr::null_mut()); count as usize];
    let got = unsafe { GetKeyboardLayoutList(Some(&mut buf)) };
    buf.truncate(got.max(0) as usize);
    buf.into_iter().map(|h| h.0 as isize).collect()
}

/// Current layout KLID (loword of the foreground thread's HKL, `{:08x}`).
#[cfg(windows)]
fn current_klid() -> Option<String> {
    let hwnd = kit::win::foreground_hwnd()?;
    let hkl = kit::win::layout_of_window(hwnd)?;
    Some(format!("{:08x}", (hkl as u32) & 0xFFFF))
}

/// Canonical 8-digit KLID: accepts `409`, `00000409`, `0x00000409`.
#[cfg(windows)]
fn normalize_klid(raw: &str) -> Option<String> {
    let raw = raw.trim();
    let raw = raw
        .strip_prefix("0x")
        .or_else(|| raw.strip_prefix("0X"))
        .unwrap_or(raw);
    u32::from_str_radix(raw, 16)
        .ok()
        .map(|v| format!("{:08x}", v & 0xFFFF))
}

#[cfg(windows)]
fn in_define_mode(mode: Option<&str>) -> bool {
    mode.is_some_and(|m| m.eq_ignore_ascii_case("InDefine"))
}

#[cfg(windows)]
fn is_plain_capslock(combo: &kit::keys::Combo) -> bool {
    combo.key == kit::keys::Key::CapsLock
        && !combo.ctrl
        && !combo.alt
        && !combo.altgr
        && !combo.shift
        && !combo.win
}

/// The next layout in the configured order; an unknown current layout starts
/// at the first entry, and the last entry wraps to the first.
#[cfg(windows)]
fn next_defined<'a>(
    current: Option<&str>,
    defined: &'a [DefinedLayout],
) -> Option<&'a DefinedLayout> {
    if defined.is_empty() {
        return None;
    }
    let current = current.and_then(normalize_klid);
    let idx = current
        .and_then(|c| {
            defined
                .iter()
                .position(|l| normalize_klid(&l.klid).as_deref() == Some(c.as_str()))
        })
        .map(|i| i + 1)
        .unwrap_or(0);
    defined.get(idx % defined.len())
}

/// The next installed HKL in the cached order; same wrap/unknown rule.
#[cfg(windows)]
fn next_installed(current: Option<u32>, installed: &[isize]) -> Option<isize> {
    if installed.is_empty() {
        return None;
    }
    let idx = current
        .and_then(|c| installed.iter().position(|h| (*h as u32) & 0xFFFF == c))
        .map(|i| i + 1)
        .unwrap_or(0);
    installed.get(idx % installed.len()).copied()
}

/// Low-level CapsLock/Esc hook. The hook only classifies and posts; the main
/// thread performs layout changes and SendInput.
#[cfg(windows)]
mod swap_hook {
    use super::{Ordering, SWAP_ESC};
    use anyhow::{Context, Result};
    use std::sync::atomic::AtomicBool;
    use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows::Win32::UI::Input::KeyboardAndMouse::*;
    use windows::Win32::UI::WindowsAndMessaging::*;

    pub const REASON_SWITCH: usize = 1;
    pub const REASON_SEND_ESC: usize = 2;

    static CAPS_DOWN: AtomicBool = AtomicBool::new(false);
    static ESC_DOWN: AtomicBool = AtomicBool::new(false);

    /// `None` = pass through; `Some(reason)` = swallow and post this reason
    /// on the first key-down. Pure so the swap table is unit-testable.
    pub(super) fn target_reason(vk: VIRTUAL_KEY, swap: bool) -> Option<usize> {
        match vk {
            VK_CAPITAL => Some(if swap { REASON_SEND_ESC } else { REASON_SWITCH }),
            VK_ESCAPE if swap => Some(REASON_SWITCH),
            _ => None,
        }
    }

    pub struct Hook(HHOOK);

    impl Drop for Hook {
        fn drop(&mut self) {
            unsafe {
                let _ = UnhookWindowsHookEx(self.0);
            }
        }
    }

    pub fn install() -> Result<Hook> {
        let hmod = unsafe {
            windows::Win32::System::LibraryLoader::GetModuleHandleW(None)
                .unwrap_or_default()
                .into()
        };
        let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(proc), Some(hmod), 0) }
            .context("CapsLock/Esc hook")?;
        Ok(Hook(hook))
    }

    unsafe extern "system" fn proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 {
            let kb = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
            // Let our own SendInput Esc through; swallowing it would undo the
            // CapsLock -> Esc mapping.
            if kb.flags & LLKHF_INJECTED == LLKHF_INJECTED {
                return unsafe { CallNextHookEx(None, code, wparam, lparam) };
            }
            let vk = VIRTUAL_KEY(kb.vkCode as u16);
            let down = matches!(wparam.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
            let up = matches!(wparam.0 as u32, WM_KEYUP | WM_SYSKEYUP);
            if down || up {
                if let Some(reason) = target_reason(vk, SWAP_ESC.load(Ordering::SeqCst)) {
                    let was_down = match vk {
                        VK_CAPITAL => CAPS_DOWN.swap(down, Ordering::SeqCst),
                        VK_ESCAPE => ESC_DOWN.swap(down, Ordering::SeqCst),
                        _ => false,
                    };
                    if down && !was_down {
                        unsafe {
                            let _ = PostMessageW(None, WM_APP, WPARAM(reason), LPARAM(0));
                        }
                    }
                    // Swallow down and up so CapsLock never toggles natively
                    // and Esc never reaches the app when it means "switch".
                    return LRESULT(1);
                }
            }
        }
        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }
}

#[cfg(windows)]
fn run() -> anyhow::Result<()> {
    use kit::keys::Combo;
    use kit::tray::{Event, Tray};
    use kit::{config, hotkey::Hotkeys, win};

    const APP: &str = "keyboard-switch";
    let hosted = std::env::args().any(|a| a == "--plugin");
    let no_tray = hosted && std::env::args().any(|a| a == "--no-tray");

    let _single = win::ensure_single_instance(APP)?;

    let (config_path, text) = config::load_profile(
        "keyboard-switch.toml",
        include_str!("../keyboard-switch.toml"),
    )?;
    let file: FileConfig = toml::from_str(&text)?;
    let settings = file.settings.unwrap_or(Settings {
        switch_mode: None,
        switch_hotkey: None,
        swap_esc_caps: None,
    });
    let mut in_define = in_define_mode(settings.switch_mode.as_deref());
    let switch_combo: Combo = settings
        .switch_hotkey
        .unwrap_or_else(|| "capslock".into())
        .parse()?;
    let mut swap_esc = settings.swap_esc_caps.unwrap_or(false);

    let is_capslock = is_plain_capslock(&switch_combo);
    SWAP_ESC.store(swap_esc, Ordering::SeqCst);
    debug_log(format!(
        "start swap={swap_esc} capslock={is_capslock} profile={}",
        config_path.display()
    ));
    let _swap_hook = if is_capslock {
        Some(swap_hook::install()?)
    } else {
        None
    };
    // CapsLock/Esc are handled by the hook; any other switch combo is a
    // normal registered hotkey.
    let hotkeys = if is_capslock {
        Hotkeys::register(&[])?
    } else {
        Hotkeys::register(&[("switch", switch_combo)])?
    };

    let mut defined: Vec<DefinedLayout> = file.layout.unwrap_or_default();
    if defined.is_empty() {
        // AHK defaults when [Layouts] is empty.
        defined = vec![
            DefinedLayout {
                klid: "00000409".into(),
                shortcut: Some("1".into()),
            },
            DefinedLayout {
                klid: "00000804".into(),
                shortcut: Some("2".into()),
            },
        ];
    }
    let mut installed: Vec<isize> = Vec::new();

    if no_tray {
        use kit::ipc::{HostCommand, PluginLink};
        let link = PluginLink::new()?;
        link.ready(
            menu_model(swap_esc, in_define),
            Some(status_text(swap_esc, in_define)),
        )?;
        kit::tray::run_headless(Some(&hotkeys), |event| match event {
            Event::Wakeup(4) => {
                while let Some(command) = link.try_recv() {
                    match command {
                        HostCommand::Invoke { id, .. } => match id.as_str() {
                            "swap_esc" => {
                                swap_esc = !swap_esc;
                                SWAP_ESC.store(swap_esc, Ordering::SeqCst);
                                persist_toggle(
                                    &text,
                                    &config_path,
                                    "swap_esc_caps",
                                    &swap_esc.to_string(),
                                );
                                let _ = link.menu(&menu_model(swap_esc, in_define));
                                let _ = link.status(&status_text(swap_esc, in_define));
                            }
                            "mode_indefine" => {
                                in_define = !in_define;
                                persist_toggle(
                                    &text,
                                    &config_path,
                                    "switch_mode",
                                    if in_define {
                                        "\"InDefine\""
                                    } else {
                                        "\"InAll\""
                                    },
                                );
                                let _ = link.menu(&menu_model(swap_esc, in_define));
                                let _ = link.status(&status_text(swap_esc, in_define));
                            }
                            _ => {}
                        },
                        HostCommand::Quit { .. } => kit::tray::quit(),
                        HostCommand::Reload { .. } => {}
                    }
                }
            }
            Event::Wakeup(reason) => match reason {
                swap_hook::REASON_SWITCH => switch_layout(in_define, &defined, &mut installed),
                swap_hook::REASON_SEND_ESC => {
                    if let Some(vk) = kit::hotkey::vk_of_key(kit::keys::Key::Escape) {
                        kit::win::send_key(vk);
                    }
                }
                _ => {}
            },
            Event::Hotkey(label) if label == "switch" => {
                switch_layout(in_define, &defined, &mut installed);
            }
            _ => {}
        });
        return Ok(());
    }

    let mut tray = Tray::new(
        APP,
        kit::icon::rgba(kit::icon::KEYBOARD_SWITCH, kit::icon::State::Normal),
    )?;
    refresh_menu(
        &mut tray,
        swap_esc,
        in_define,
        is_capslock,
        &switch_combo,
        &defined,
        &mut installed,
        hosted,
    );

    tray.run(Some(&hotkeys), |tray, event| {
        match event {
            Event::Wakeup(reason) => {
                debug_log(format!("wakeup reason={reason}"));
                match reason {
                    swap_hook::REASON_SWITCH => {
                        switch_layout(in_define, &defined, &mut installed);
                    }
                    swap_hook::REASON_SEND_ESC => {
                        if let Some(vk) = kit::hotkey::vk_of_key(kit::keys::Key::Escape) {
                            kit::win::send_key(vk);
                        }
                    }
                    _ => {}
                }
            }
            Event::Hotkey(label) if label == "switch" => {
                switch_layout(in_define, &defined, &mut installed);
            }
            Event::MenuId(id) => match id.as_str() {
                "swap_esc" => {
                    swap_esc = !swap_esc;
                    SWAP_ESC.store(swap_esc, Ordering::SeqCst);
                    persist_toggle(&text, &config_path, "swap_esc_caps", &swap_esc.to_string());
                }
                "mode_indefine" => {
                    in_define = !in_define;
                    let v = if in_define {
                        "\"InDefine\""
                    } else {
                        "\"InAll\""
                    };
                    persist_toggle(&text, &config_path, "switch_mode", v);
                }
                _ => {}
            },
            Event::Menu(label) => match label.as_str() {
                "Start with Windows" => {
                    let now = !win::autostart_enabled(APP);
                    if let Err(e) = win::set_autostart(APP, now) {
                        win::message_box(APP, &format!("Autostart failed:\n{e:#}"));
                    }
                }
                "Edit Config" => win::edit_config(&config_path),
                "Reload" => {
                    config::restart_self();
                    tray.quit();
                }
                "Exit" => tray.quit(),
                _ => {}
            },
            _ => {}
        }
        refresh_menu(
            tray,
            swap_esc,
            in_define,
            is_capslock,
            &switch_combo,
            &defined,
            &mut installed,
            hosted,
        );
    });
    Ok(())
}

#[cfg(windows)]
fn menu_model(swap: bool, in_define: bool) -> kit::menu::MenuModel {
    use kit::menu::{MenuItem, MenuModel};
    MenuModel::new(vec![
        MenuItem::Toggle {
            id: "swap_esc".into(),
            label: "Swap ESC/CapsLock".into(),
            checked: swap,
            enabled: true,
        },
        MenuItem::Toggle {
            id: "mode_indefine".into(),
            label: "Switch Mode: InDefine".into(),
            checked: in_define,
            enabled: true,
        },
    ])
}

#[cfg(windows)]
fn status_text(swap: bool, in_define: bool) -> String {
    format!(
        "swap={} mode={}",
        if swap { "on" } else { "off" },
        if in_define { "InDefine" } else { "InAll" }
    )
}

/// Rewrite one `key = ...` line in the config file, preserving comments.
/// No-op when the key is absent (e.g. user deleted it).
#[cfg(windows)]
fn persist_toggle(original: &str, path: &std::path::Path, key: &str, value: &str) {
    let current = std::fs::read_to_string(path).unwrap_or_else(|_| original.to_string());
    let new_line = format!("{key} = {value}");
    let rewritten = current
        .lines()
        .map(|l| {
            let trimmed = l.trim_start();
            if trimmed.starts_with(&format!("{key} =")) || trimmed == key {
                new_line.as_str()
            } else {
                l
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let _ = std::fs::write(path, rewritten);
}

#[cfg(windows)]
fn switch_layout(in_define: bool, defined: &[DefinedLayout], installed: &mut Vec<isize>) {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::PostMessageW;
    const WM_INPUTLANGCHANGEREQUEST: u32 = 0x50;

    let Some(hwnd) = kit::win::foreground_hwnd() else {
        return;
    };
    let current = kit::win::layout_of_window(hwnd).unwrap_or(0);
    debug_log(format!(
        "switch in_define={in_define} current={:08x}",
        (current as u32) & 0xFFFF
    ));

    if in_define {
        let cur_klid = format!("{:08x}", (current as u32) & 0xFFFF);
        let Some(next) = next_defined(Some(&cur_klid), defined) else {
            return;
        };
        // AHK SetKeyboardLayoutShortcut: Alt+Shift+<digit>.
        let Some(digit) = next.shortcut.as_deref().and_then(|s| s.chars().next()) else {
            return;
        };
        let Some(vk) = kit::hotkey::vk_of_key(kit::keys::Key::Char(digit)) else {
            return;
        };
        debug_log(format!("next defined klid={} shortcut={digit}", next.klid));
        send_alt_shift(vk);
    } else {
        if installed.is_empty() {
            *installed = installed_layouts();
        }
        let Some(next) = next_installed(Some((current as u32) & 0xFFFF), installed) else {
            return;
        };
        debug_log(format!("next installed hkl={next:08x}"));
        unsafe {
            let _ = PostMessageW(
                Some(hwnd),
                WM_INPUTLANGCHANGEREQUEST,
                WPARAM(0),
                LPARAM(next),
            );
        }
    }
}

#[cfg(windows)]
fn send_alt_shift(vk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY) {
    use windows::Win32::UI::Input::KeyboardAndMouse::*;
    fn inp(vk: VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }
    let down = KEYBD_EVENT_FLAGS(0);
    let inputs = [
        inp(VK_LMENU, down),
        inp(VK_LSHIFT, down),
        inp(vk, down),
        inp(vk, KEYEVENTF_KEYUP),
        inp(VK_LSHIFT, KEYEVENTF_KEYUP),
        inp(VK_LMENU, KEYEVENTF_KEYUP),
    ];
    unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
}

#[cfg(windows)]
#[allow(clippy::too_many_arguments)] // tray + full app state; a struct adds no clarity here
fn refresh_menu(
    tray: &mut kit::tray::Tray,
    swap: bool,
    in_define: bool,
    is_capslock: bool,
    switch_combo: &kit::keys::Combo,
    defined: &[DefinedLayout],
    installed: &mut Vec<isize>,
    hosted: bool,
) {
    use kit::tray::Entry;

    // Tooltip (AHK UpdateTrayTip).
    let current = current_klid().unwrap_or_default();
    let mut tip = String::from("keyboard-switch\nSwitch Hotkey: ");
    if swap && is_capslock {
        tip.push_str(&format!("ESC ({switch_combo}->ESC)"));
    } else {
        tip.push_str(&switch_combo.to_string());
    }
    tip.push_str(if in_define {
        "\nDefined layouts:"
    } else {
        "\nInstalled layouts:"
    });
    let klids: Vec<String> = if in_define {
        defined
            .iter()
            .map(|l| normalize_klid(&l.klid).unwrap_or_else(|| l.klid.trim().to_ascii_lowercase()))
            .collect()
    } else {
        if installed.is_empty() {
            *installed = installed_layouts();
        }
        installed
            .iter()
            .map(|h| format!("{:08x}", (*h as u32) & 0xFFFF))
            .collect()
    };
    for k in &klids {
        let name = u32::from_str_radix(k, 16)
            .ok()
            .and_then(kit::win::layout_text)
            .unwrap_or_else(|| k.clone());
        if *k == current {
            tip.push_str(&format!("\n> {name}"));
        } else {
            tip.push_str(&format!("\n  {name}"));
        }
    }
    tray.set_tooltip(&tip);

    let mut entries = menu_model(swap, in_define).render();
    if !hosted {
        entries.push(Entry::Toggle(
            "Start with Windows",
            kit::win::autostart_enabled("keyboard-switch"),
        ));
    }
    entries.push(Entry::Sep);
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

    fn defined(klids: &[&str]) -> Vec<DefinedLayout> {
        klids
            .iter()
            .map(|klid| DefinedLayout {
                klid: (*klid).into(),
                shortcut: None,
            })
            .collect()
    }

    #[test]
    fn klids_are_normalized() {
        assert_eq!(normalize_klid("409").as_deref(), Some("00000409"));
        assert_eq!(normalize_klid("0x00000409").as_deref(), Some("00000409"));
        assert_eq!(normalize_klid(" 00000804 ").as_deref(), Some("00000804"));
        assert_eq!(normalize_klid("zz"), None);
        assert_eq!(normalize_klid("0x"), None);
    }

    #[test]
    fn defined_cycle_wraps_and_starts_at_first_when_unknown() {
        let list = defined(&["00000409", "00000804", "00000411"]);
        assert_eq!(
            next_defined(Some("00000409"), &list).unwrap().klid,
            "00000804"
        );
        assert_eq!(
            next_defined(Some("00000411"), &list).unwrap().klid,
            "00000409"
        );
        assert_eq!(
            next_defined(Some("deadbeef"), &list).unwrap().klid,
            "00000409"
        );
        assert_eq!(next_defined(None, &list).unwrap().klid, "00000409");
        assert!(next_defined(None, &[]).is_none());
    }

    #[test]
    fn installed_cycle_wraps_and_starts_at_first_when_unknown() {
        let list = [0x0409isize, 0x0804, 0x0411];
        assert_eq!(next_installed(Some(0x0409), &list), Some(0x0804));
        assert_eq!(next_installed(Some(0x0411), &list), Some(0x0409));
        assert_eq!(next_installed(Some(0x9999), &list), Some(0x0409));
        assert_eq!(next_installed(None, &list), Some(0x0409));
        assert_eq!(next_installed(None, &[]), None);
    }

    #[test]
    fn capslock_detection_requires_no_modifiers() {
        assert!(is_plain_capslock(&"capslock".parse().unwrap()));
        assert!(!is_plain_capslock(&"ctrl+capslock".parse().unwrap()));
        assert!(!is_plain_capslock(&"altgr+capslock".parse().unwrap()));
        assert!(!is_plain_capslock(&"esc".parse().unwrap()));
    }

    #[test]
    fn switch_mode_is_case_insensitive() {
        assert!(in_define_mode(Some("InDefine")));
        assert!(in_define_mode(Some("indefine")));
        assert!(!in_define_mode(Some("InAll")));
        assert!(!in_define_mode(None));
    }

    #[test]
    fn hook_covers_the_swap_table() {
        use windows::Win32::UI::Input::KeyboardAndMouse::{VK_CAPITAL, VK_ESCAPE};
        assert_eq!(
            swap_hook::target_reason(VK_CAPITAL, false),
            Some(swap_hook::REASON_SWITCH)
        );
        assert_eq!(
            swap_hook::target_reason(VK_CAPITAL, true),
            Some(swap_hook::REASON_SEND_ESC)
        );
        assert_eq!(
            swap_hook::target_reason(VK_ESCAPE, true),
            Some(swap_hook::REASON_SWITCH)
        );
        assert_eq!(swap_hook::target_reason(VK_ESCAPE, false), None);
        assert_eq!(
            swap_hook::target_reason(windows::Win32::UI::Input::KeyboardAndMouse::VK_A, true),
            None
        );
    }
}
