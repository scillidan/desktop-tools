#![cfg_attr(windows, windows_subsystem = "windows")]

//! goldendict-auto-search: bind windows (hotkey on the window under the
//! cursor) so double-clicking a word copies+queries GoldenDict, and/or so
//! clipboard changes inside bound windows query it.
//! Ported from AHK-GoldenDictAutoSearch.
//!
//! Parity notes:
//! - The AHK original clears the clipboard, sends ^c, ClipWaits 500 ms, then
//!   restores ClipboardAll. We snapshot the previous text, send ^c, wait
//!   (<=500 ms) for the clipboard sequence to change, then restore the text.
//!   Restoring text covers the normal word-selection use; images/files on the
//!   clipboard are not restored yet (documented limitation).
//! - The 500 ms clipboard poll timer becomes a thread posting wakeups.
//! - Bindings are in-memory only, as in the AHK original.

#[cfg(windows)]
use kit::win;
#[cfg(windows)]
use std::collections::HashSet;
#[cfg(windows)]
use std::path::Path;
#[cfg(windows)]
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
#[cfg(windows)]
use std::sync::{Arc, RwLock};

fn main() -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        if let Err(e) = run() {
            kit::win::fatal_error("goldendict-auto-search", &format!("Error:\n{e:#}"));
        }
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        kit::unimplemented("goldendict-auto-search")
    }
}

#[cfg(windows)]
fn epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Opt-in trace: `KBTOOLS_DEBUG=1` appends to
/// `%TEMP%\goldendict-auto-search.log`. Never logs clipboard text, only
/// lengths and process/group names.
#[cfg(windows)]
fn debug_log(msg: impl AsRef<str>) {
    if std::env::var_os("KBTOOLS_DEBUG").is_none() {
        return;
    }
    use std::io::Write;
    let path = std::env::temp_dir().join("goldendict-auto-search.log");
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "{}", msg.as_ref());
    }
}

/// AHK `now - LastClickTime < 300`.
#[cfg(windows)]
fn is_double_click(now_ms: u64, last_ms: u64) -> bool {
    now_ms.saturating_sub(last_ms) < 300
}

/// AHK `A_TickCount - ClipGuardTick < 1500`.
#[cfg(windows)]
fn in_clip_guard(now_ms: u64, guard_ms: u64) -> bool {
    now_ms.saturating_sub(guard_ms) < 1500
}

/// AHK `StrLen(text) > 1000`.
#[cfg(windows)]
fn too_long(text: &str) -> bool {
    text.chars().count() > 1000
}

/// Bind/unbind a process name; returns true when it is now bound.
#[cfg(windows)]
fn toggle_binding(bound: &mut std::collections::HashSet<String>, name: &str) -> bool {
    if bound.remove(name) {
        false
    } else {
        bound.insert(name.to_string());
        true
    }
}

#[cfg(windows)]
fn clipboard_text() -> String {
    arboard::Clipboard::new()
        .ok()
        .and_then(|mut cb| cb.get_text().ok())
        .unwrap_or_default()
}

/// Replace `key = value` in the current text, inserting it under `[section]`
/// when the user removed the line.
#[cfg(windows)]
fn set_key_line(text: &str, section: &str, key: &str, value: &str) -> String {
    let new_line = format!("{key} = {value}");
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let mut replaced = false;
    for line in &mut lines {
        let found = line.trim_start().split('=').next().unwrap_or("").trim();
        if found == key {
            *line = new_line.clone();
            replaced = true;
            break;
        }
    }
    if !replaced {
        let header = format!("[{section}]");
        if let Some(pos) = lines.iter().position(|l| l.trim() == header) {
            lines.insert(pos + 1, new_line);
        } else {
            lines.push(header);
            lines.push(new_line);
        }
    }
    lines.join("\n")
}

#[cfg(windows)]
#[derive(serde::Deserialize)]
struct GdSection {
    executable: Option<String>,
    double_click_group: Option<String>,
    clipboard_group: Option<String>,
    clipboard_hotkey_enabled: Option<bool>,
}

#[cfg(windows)]
#[derive(serde::Deserialize)]
struct SettingsSection {
    double_click_enabled: Option<bool>,
    clipboard_enabled: Option<bool>,
}

#[cfg(windows)]
#[derive(serde::Deserialize)]
struct HotkeysSection {
    toggle_key: Option<String>,
}

#[cfg(windows)]
#[derive(serde::Deserialize)]
struct FileConfig {
    goldendict: Option<GdSection>,
    settings: Option<SettingsSection>,
    hotkeys: Option<HotkeysSection>,
}

#[cfg(windows)]
#[allow(clippy::too_many_arguments)]
fn on_left_up(
    dc_enabled: bool,
    last_click_ms: &mut u64,
    bound: &Arc<RwLock<HashSet<String>>>,
    clip_guard_ms: &Arc<AtomicU64>,
    cb_seq: &Arc<AtomicU32>,
    popup: &Arc<AtomicBool>,
    dc_group: &str,
    exe: &str,
    notifier: Option<&kit::ipc::PluginNotifier>,
) {
    if !dc_enabled {
        return;
    }
    let fg_bound = win::foreground_hwnd()
        .and_then(win::process_name)
        .is_some_and(|name| bound.read().map(|b| b.contains(&name)).unwrap_or(false));
    if !fg_bound {
        return;
    }
    let now = epoch_ms();
    let is_double = is_double_click(now, *last_click_ms);
    *last_click_ms = now;
    debug_log(format!("click double={is_double}"));
    if !is_double {
        return;
    }
    std::thread::sleep(std::time::Duration::from_millis(50));
    clip_guard_ms.store(epoch_ms(), Ordering::SeqCst);
    let old_text = clipboard_text();
    let before = win::clipboard_sequence();
    win::send_ctrl_c();
    let mut text = String::new();
    for _ in 0..20 {
        std::thread::sleep(std::time::Duration::from_millis(25));
        if win::clipboard_sequence() != before {
            text = clipboard_text();
            break;
        }
    }
    let text = text.trim().to_string();
    let restored = if old_text.is_empty() {
        false
    } else {
        arboard::Clipboard::new()
            .ok()
            .is_some_and(|mut cb| cb.set_text(old_text.clone()).is_ok())
    };
    debug_log(format!(
        "copy seq={before}->{} len={} restored={restored}",
        win::clipboard_sequence(),
        text.chars().count()
    ));
    cb_seq.store(win::clipboard_sequence(), Ordering::SeqCst);
    if text.is_empty() || too_long(&text) {
        return;
    }
    search_with_notify(exe, dc_group, popup.load(Ordering::SeqCst), &text, notifier);
}

#[cfg(windows)]
#[allow(clippy::too_many_arguments)]
fn handle_action(
    id: &str,
    dc_enabled: &mut bool,
    cb_enabled: &Arc<AtomicBool>,
    cb_seq: &Arc<AtomicU32>,
    popup: &Arc<AtomicBool>,
    bound: &Arc<RwLock<HashSet<String>>>,
    exe: &str,
    cb_group: &str,
    text: &str,
    config_path: &Path,
    notifier: Option<&kit::ipc::PluginNotifier>,
) -> Option<String> {
    match id {
        "double_click" => {
            *dc_enabled = !*dc_enabled;
            persist(
                text,
                config_path,
                "settings",
                "double_click_enabled",
                &dc_enabled.to_string(),
                notifier,
            );
            Some(format!(
                "DoubleClick Mode: {}",
                if *dc_enabled { "On" } else { "Off" }
            ))
        }
        "clipboard" => {
            let now = !cb_enabled.load(Ordering::SeqCst);
            cb_enabled.store(now, Ordering::SeqCst);
            if now {
                cb_seq.store(win::clipboard_sequence(), Ordering::SeqCst);
            }
            persist(
                text,
                config_path,
                "settings",
                "clipboard_enabled",
                &now.to_string(),
                notifier,
            );
            Some(if now {
                format!("Clipboard Mode: On (group {cb_group})")
            } else {
                "Clipboard Mode: Off".into()
            })
        }
        "test_clipboard" => {
            let query = clipboard_text().trim().to_string();
            if query.is_empty() {
                Some("Clipboard is empty".into())
            } else if too_long(&query) {
                Some("Clipboard text is over 1000 characters".into())
            } else {
                search_with_notify(
                    exe,
                    cb_group,
                    popup.load(Ordering::SeqCst),
                    &query,
                    notifier,
                );
                None
            }
        }
        "clear_bound" => {
            bound.write().unwrap().clear();
            Some("All bindings cleared".into())
        }
        "popup" => {
            let now = !popup.load(Ordering::SeqCst);
            popup.store(now, Ordering::SeqCst);
            persist(
                text,
                config_path,
                "goldendict",
                "clipboard_hotkey_enabled",
                &now.to_string(),
                notifier,
            );
            Some(format!("Popup Search: {}", if now { "On" } else { "Off" }))
        }
        _ => None,
    }
}

#[cfg(windows)]
fn menu_model(
    dc: bool,
    cb: bool,
    popup: bool,
    bound: &Arc<RwLock<HashSet<String>>>,
    cb_group: &str,
) -> kit::menu::MenuModel {
    use kit::menu::{MenuItem, MenuModel};
    let mut names: Vec<String> = bound
        .read()
        .map(|b| b.iter().cloned().collect())
        .unwrap_or_default();
    names.sort();
    let listed = names.iter().take(3).cloned().collect::<Vec<_>>().join(", ");
    let summary = if names.is_empty() {
        "Bind Windows [0]".to_string()
    } else if names.len() > 3 {
        format!("Bind Windows [{}]: {listed}, ...", names.len())
    } else {
        format!("Bind Windows [{}]: {listed}", names.len())
    };
    MenuModel::new(vec![
        MenuItem::Info { label: summary },
        MenuItem::Toggle {
            id: "double_click".into(),
            label: "DoubleClick Mode".into(),
            checked: dc,
            enabled: true,
        },
        MenuItem::Toggle {
            id: "clipboard".into(),
            label: "Clipboard Mode".into(),
            checked: cb,
            enabled: true,
        },
        MenuItem::Info {
            label: format!(
                "Clipboard Group: {}",
                if cb_group.is_empty() {
                    "all dictionaries"
                } else {
                    cb_group
                }
            ),
        },
        MenuItem::Item {
            id: "test_clipboard".into(),
            label: "Test Clipboard Search".into(),
            enabled: true,
        },
        MenuItem::Separator,
        MenuItem::Item {
            id: "clear_bound".into(),
            label: "Clear All Bound Windows".into(),
            enabled: true,
        },
        MenuItem::Toggle {
            id: "popup".into(),
            label: "Clipboard Hotkey (Popup Search)".into(),
            checked: popup,
            enabled: true,
        },
    ])
}

#[cfg(windows)]
fn status_text(dc: bool, cb: bool, popup: bool, bound: &Arc<RwLock<HashSet<String>>>) -> String {
    let count = bound.read().map(|b| b.len()).unwrap_or(0);
    format!(
        "bound={count} dc={} cb={} popup={}",
        if dc { "on" } else { "off" },
        if cb { "on" } else { "off" },
        if popup { "on" } else { "off" }
    )
}

#[cfg(windows)]
fn run() -> anyhow::Result<()> {
    use kit::keys::Combo;
    use kit::tray::{Event, Tray};
    use kit::{config, hotkey::Hotkeys, win};
    use std::collections::HashSet;
    use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
    use std::sync::{Arc, RwLock};
    use std::time::Duration;

    const APP: &str = "goldendict-auto-search";
    let hosted = std::env::args().any(|a| a == "--plugin");
    let no_tray = hosted && std::env::args().any(|a| a == "--no-tray");

    let _single = win::ensure_single_instance(APP)?;

    let (config_path, text) = config::load_layered_profile(
        "goldendict-auto-search.toml",
        include_str!("../goldendict-auto-search.toml"),
    )?;
    let file: FileConfig = toml::from_str(&text)?;
    let gd = file.goldendict.unwrap_or(GdSection {
        executable: None,
        double_click_group: None,
        clipboard_group: None,
        clipboard_hotkey_enabled: None,
    });
    let st = file.settings.unwrap_or(SettingsSection {
        double_click_enabled: None,
        clipboard_enabled: None,
    });
    let hk = file.hotkeys.unwrap_or(HotkeysSection { toggle_key: None });

    let exe = gd.executable.unwrap_or_else(|| "goldendict".into());
    let dc_group = gd.double_click_group.unwrap_or_default();
    let cb_group = gd.clipboard_group.unwrap_or_default();
    let popup = Arc::new(AtomicBool::new(
        gd.clipboard_hotkey_enabled.unwrap_or(false),
    ));
    let mut dc_enabled = st.double_click_enabled.unwrap_or(true);
    let cb_enabled_cfg = st.clipboard_enabled.unwrap_or(false);
    let toggle_combo: Combo = hk
        .toggle_key
        .unwrap_or_else(|| "ctrl+alt+shift+g".into())
        .parse()?;

    // Shared state between the tray loop and helper threads.
    let bound: Arc<RwLock<HashSet<String>>> = Arc::new(RwLock::new(HashSet::new()));
    let clip_guard_ms = Arc::new(AtomicU64::new(0)); // AHK ClipGuardTick
    let cb_enabled = Arc::new(AtomicBool::new(cb_enabled_cfg));
    // Clipboard sequence last handled by Clipboard Mode. Sequence numbers
    // detect a repeat copy of the same word, which text comparison misses.
    // A double-click also advances this after restoring the old clipboard so
    // the monitor does not search its own copy after the guard.
    let cb_seq = Arc::new(AtomicU32::new(win::clipboard_sequence()));
    debug_log(format!(
        "start dc={dc_enabled} cb={cb_enabled_cfg} popup={} profile={}",
        popup.load(Ordering::SeqCst),
        config_path.display()
    ));

    // Passive mouse hook: report every left-button release to the loop
    // (AHK `~LButton Up`). wparam 1 = "button released".
    let _mouse_hook = mouse::install()?;

    let link = if no_tray {
        Some(kit::ipc::PluginLink::new()?)
    } else {
        None
    };
    let notifier = link.as_ref().map(kit::ipc::PluginLink::notifier);

    // Clipboard monitor thread (AHK ClipMonitorTimer, 500 ms).
    {
        let notifier = notifier.clone();
        let bound = bound.clone();
        let clip_guard_ms = clip_guard_ms.clone();
        let cb_enabled = cb_enabled.clone();
        let cb_seq = cb_seq.clone();
        let popup = popup.clone();
        let exe = exe.clone();
        let cb_group = cb_group.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(500));
            if !cb_enabled.load(Ordering::SeqCst) {
                continue;
            }
            if in_clip_guard(epoch_ms(), clip_guard_ms.load(Ordering::SeqCst)) {
                continue;
            }
            let seq = win::clipboard_sequence();
            if seq == cb_seq.load(Ordering::SeqCst) {
                continue;
            }
            cb_seq.store(seq, Ordering::SeqCst);
            let clip_text = clipboard_text().trim().to_string();
            let fg_name = win::foreground_hwnd().and_then(win::process_name);
            let fg_bound = fg_name
                .as_ref()
                .is_some_and(|name| bound.read().map(|b| b.contains(name)).unwrap_or(false));
            if !fg_bound {
                debug_log(format!(
                    "clipboard changed outside bound fg={}",
                    fg_name.as_deref().unwrap_or("?")
                ));
                continue;
            }
            if clip_text.is_empty() || too_long(&clip_text) {
                debug_log(format!(
                    "clipboard skipped len={} fg={}",
                    clip_text.chars().count(),
                    fg_name.as_deref().unwrap_or("?")
                ));
                continue;
            }
            debug_log(format!(
                "clipboard search len={} fg={}",
                clip_text.chars().count(),
                fg_name.as_deref().unwrap_or("?")
            ));
            search_with_notify(
                &exe,
                &cb_group,
                popup.load(Ordering::SeqCst),
                &clip_text,
                notifier.as_ref(),
            );
        });
    }

    // Tooltip refresh thread (AHK UpdateTrayTip, 1000 ms). wparam 3.
    {
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(1000));
            kit::tray::post_wakeup(3);
        });
    }

    let hotkeys = Hotkeys::register(&[("toggle", toggle_combo)])?;
    let mut last_click_ms: u64 = 0; // AHK LastClickTime

    if no_tray {
        use kit::ipc::HostCommand;
        let link = link.expect("headless mode has a plugin link");
        let send_state = |dc: bool, cb: bool, popup_on: bool| {
            let _ = link.menu(&menu_model(dc, cb, popup_on, &bound, &cb_group));
            let _ = link.status(&status_text(dc, cb, popup_on, &bound));
        };
        link.ready(
            menu_model(
                dc_enabled,
                cb_enabled.load(Ordering::SeqCst),
                popup.load(Ordering::SeqCst),
                &bound,
                &cb_group,
            ),
            Some(status_text(
                dc_enabled,
                cb_enabled.load(Ordering::SeqCst),
                popup.load(Ordering::SeqCst),
                &bound,
            )),
        )?;
        kit::tray::run_headless(Some(&hotkeys), |event| match event {
            Event::Hotkey(label) if label == "toggle" => {
                let Some(hwnd) = win::hwnd_under_cursor() else {
                    let _ = link.status("No window under cursor");
                    let _ = link.notify(APP, "No window under cursor");
                    return;
                };
                let Some(name) = win::process_name(hwnd) else {
                    let _ = link.status("Cannot get window info");
                    let _ = link.notify(APP, "Cannot get window info");
                    return;
                };
                let now_bound = {
                    let mut b = bound.write().unwrap();
                    toggle_binding(&mut b, &name)
                };
                debug_log(format!("bind {name} -> {now_bound}"));
                let _ = link.notify(
                    APP,
                    if now_bound {
                        format!("Bound: {name}")
                    } else {
                        format!("Unbound: {name}")
                    },
                );
                send_state(
                    dc_enabled,
                    cb_enabled.load(Ordering::SeqCst),
                    popup.load(Ordering::SeqCst),
                );
            }
            Event::Wakeup(1) => on_left_up(
                dc_enabled,
                &mut last_click_ms,
                &bound,
                &clip_guard_ms,
                &cb_seq,
                &popup,
                &dc_group,
                &exe,
                notifier.as_ref(),
            ),
            Event::Wakeup(4) => {
                while let Some(command) = link.try_recv() {
                    match command {
                        HostCommand::Invoke { id, .. } => {
                            let message = handle_action(
                                &id,
                                &mut dc_enabled,
                                &cb_enabled,
                                &cb_seq,
                                &popup,
                                &bound,
                                &exe,
                                &cb_group,
                                &text,
                                &config_path,
                                notifier.as_ref(),
                            );
                            if let Some(message) = message {
                                let _ = link.notify(APP, message);
                            }
                            send_state(
                                dc_enabled,
                                cb_enabled.load(Ordering::SeqCst),
                                popup.load(Ordering::SeqCst),
                            );
                        }
                        HostCommand::Quit { .. } => kit::tray::quit(),
                        HostCommand::Reload { .. } => {}
                    }
                }
            }
            _ => {}
        });
        return Ok(());
    }

    let mut tray = Tray::new(
        APP,
        kit::icon::rgba(kit::icon::GOLDENDICT_AUTO_SEARCH, kit::icon::State::Normal),
    )?;
    refresh_menu(
        &mut tray,
        dc_enabled,
        cb_enabled.load(Ordering::SeqCst),
        popup.load(Ordering::SeqCst),
        &cb_group,
        &bound,
        hosted,
    );

    tray.run(Some(&hotkeys), |tray, event| {
        match event {
            Event::Hotkey(label) if label == "toggle" => {
                let Some(hwnd) = win::hwnd_under_cursor() else {
                    win::center_tip("No window under cursor", Duration::from_millis(1500));
                    return;
                };
                let Some(name) = win::process_name(hwnd) else {
                    win::center_tip("Cannot get window info", Duration::from_millis(1500));
                    return;
                };
                let mut b = bound.write().unwrap();
                let now_bound = toggle_binding(&mut b, &name);
                drop(b);
                debug_log(format!("bind {name} -> {now_bound}"));
                let msg = if now_bound {
                    format!("Bound: {name}")
                } else {
                    format!("Unbound: {name}")
                };
                win::center_tip(&msg, Duration::from_millis(1500));
                let _ = tray.notify(APP, &msg);
                update_tooltip(tray, &toggle_combo, &bound);
                refresh_menu(
                    tray,
                    dc_enabled,
                    cb_enabled.load(Ordering::SeqCst),
                    popup.load(Ordering::SeqCst),
                    &cb_group,
                    &bound,
                    hosted,
                );
            }
            Event::Wakeup(1) => {
                // Left button released (AHK OnLButtonUp).
                if !dc_enabled {
                    return;
                }
                let fg_bound = win::foreground_hwnd()
                    .and_then(win::process_name)
                    .is_some_and(|name| bound.read().map(|b| b.contains(&name)).unwrap_or(false));
                if !fg_bound {
                    return;
                }
                let now = epoch_ms();
                let is_double = is_double_click(now, last_click_ms);
                last_click_ms = now;
                debug_log(format!("click double={is_double}"));
                if !is_double {
                    return;
                }
                std::thread::sleep(Duration::from_millis(50)); // AHK Sleep 50
                clip_guard_ms.store(epoch_ms(), Ordering::SeqCst);
                let old_text = clipboard_text();
                let before = win::clipboard_sequence();
                win::send_ctrl_c();
                // ClipWait 0.5: wait for the clipboard sequence to change,
                // not for different text. A copy of the same word is still a
                // copy, and must search.
                let mut text = String::new();
                for _ in 0..20 {
                    std::thread::sleep(Duration::from_millis(25));
                    if win::clipboard_sequence() != before {
                        text = clipboard_text();
                        break;
                    }
                }
                let text = text.trim().to_string();
                // Restore before the validity checks: a copy timeout or an
                // over-long selection must not leave the clipboard changed.
                // Text is best-effort; images/files are a documented v1
                // limitation.
                let restored = if old_text.is_empty() {
                    false
                } else {
                    arboard::Clipboard::new()
                        .ok()
                        .is_some_and(|mut cb| cb.set_text(old_text.clone()).is_ok())
                };
                // Mark the final clipboard state as handled so Clipboard Mode
                // does not search it again after the guard expires.
                debug_log(format!(
                    "copy seq={before}->{} len={} restored={restored}",
                    win::clipboard_sequence(),
                    text.chars().count()
                ));
                cb_seq.store(win::clipboard_sequence(), Ordering::SeqCst);
                if text.is_empty() || too_long(&text) {
                    return;
                }
                search(&exe, &dc_group, popup.load(Ordering::SeqCst), &text);
            }
            Event::Wakeup(3) => {
                update_tooltip(tray, &toggle_combo, &bound);
            }
            Event::MenuId(id) => {
                if let Some(msg) = handle_action(
                    &id,
                    &mut dc_enabled,
                    &cb_enabled,
                    &cb_seq,
                    &popup,
                    &bound,
                    &exe,
                    &cb_group,
                    &text,
                    &config_path,
                    None,
                ) {
                    win::center_tip(&msg, Duration::from_millis(1500));
                    let _ = tray.notify(APP, &msg);
                }
                update_tooltip(tray, &toggle_combo, &bound);
                refresh_menu(
                    tray,
                    dc_enabled,
                    cb_enabled.load(Ordering::SeqCst),
                    popup.load(Ordering::SeqCst),
                    &cb_group,
                    &bound,
                    hosted,
                );
            }
            Event::Menu(label) => {
                match label.as_str() {
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
                }
                refresh_menu(
                    tray,
                    dc_enabled,
                    cb_enabled.load(Ordering::SeqCst),
                    popup.load(Ordering::SeqCst),
                    &cb_group,
                    &bound,
                    hosted,
                );
            }
            _ => {}
        }
    });
    Ok(())
}

#[cfg(windows)]
fn search(exe: &str, group: &str, popup: bool, query: &str) {
    search_with_notify(exe, group, popup, query, None);
}

#[cfg(windows)]
fn search_args(group: &str, popup: bool, query: &str) -> Vec<String> {
    let mut args = Vec::with_capacity(2);
    if !group.is_empty() {
        args.push(if popup {
            format!("--popup-group-name={group}")
        } else {
            format!("--group-name={group}")
        });
    }
    args.push(query.to_string());
    args
}

#[cfg(windows)]
fn search_with_notify(
    exe: &str,
    group: &str,
    popup: bool,
    query: &str,
    notifier: Option<&kit::ipc::PluginNotifier>,
) {
    debug_log(format!(
        "search group={group} popup={popup} len={}",
        query.chars().count()
    ));
    let args = search_args(group, popup, query);
    if let Err(e) = std::process::Command::new(exe).args(args).spawn() {
        let message = format!("Cannot start GoldenDict:\n{e:#}\n\nExecutable: {exe}");
        if let Some(notifier) = notifier {
            let _ = notifier.notify("goldendict-auto-search", message);
        } else {
            kit::win::message_box("goldendict-auto-search", &message);
        }
    }
}

#[cfg(windows)]
fn persist(
    original: &str,
    path: &std::path::Path,
    section: &str,
    key: &str,
    value: &str,
    notifier: Option<&kit::ipc::PluginNotifier>,
) {
    let current = std::fs::read_to_string(path).unwrap_or_else(|_| original.to_string());
    let updated = set_key_line(&current, section, key, value);
    if let Err(e) = std::fs::write(path, updated) {
        let message = format!("Cannot save config:\n{e:#}");
        if let Some(notifier) = notifier {
            let _ = notifier.notify("goldendict-auto-search", message);
        } else {
            kit::win::message_box("goldendict-auto-search", &message);
        }
    }
}

#[cfg(windows)]
fn update_tooltip(
    tray: &mut kit::tray::Tray,
    toggle: &kit::keys::Combo,
    bound: &std::sync::RwLock<std::collections::HashSet<String>>,
) {
    let mut tip = format!("goldendict-auto-search\nToggle Key: {toggle}");
    let list: Vec<String> = bound
        .read()
        .map(|b| b.iter().cloned().collect())
        .unwrap_or_default();
    if !list.is_empty() {
        let mut sorted = list;
        sorted.sort();
        tip.push_str(&format!("\nBind Windows [{}]", sorted.len()));
        for exe in sorted {
            tip.push_str(&format!("\n  {exe}"));
        }
    }
    tray.set_tooltip(&tip);
}

#[cfg(windows)]
fn refresh_menu(
    tray: &mut kit::tray::Tray,
    dc: bool,
    cb: bool,
    popup: bool,
    cb_group: &str,
    bound: &Arc<RwLock<HashSet<String>>>,
    hosted: bool,
) {
    use kit::tray::Entry;
    let mut entries = menu_model(dc, cb, popup, bound, cb_group).render();
    if !hosted {
        entries.push(Entry::Toggle(
            "Start with Windows",
            kit::win::autostart_enabled("goldendict-auto-search"),
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

#[cfg(windows)]
mod mouse {
    use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, SetWindowsHookExW, UnhookWindowsHookEx, HHOOK, WH_MOUSE_LL, WM_LBUTTONUP,
    };

    pub struct MouseHook(HHOOK);

    impl Drop for MouseHook {
        fn drop(&mut self) {
            unsafe {
                let _ = UnhookWindowsHookEx(self.0);
            }
        }
    }

    /// Passive `WH_MOUSE_LL` hook: forwards every left-button release to the
    /// tray loop as `WM_APP` wparam 1 (never swallows).
    pub fn install() -> anyhow::Result<MouseHook> {
        let hmod = unsafe {
            windows::Win32::System::LibraryLoader::GetModuleHandleW(None)
                .unwrap_or_default()
                .into()
        };
        let hook = unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(proc), Some(hmod), 0) }?;
        Ok(MouseHook(hook))
    }

    unsafe extern "system" fn proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 && wparam.0 as u32 == WM_LBUTTONUP {
            unsafe {
                use windows::Win32::Foundation::{LPARAM as LP, WPARAM as WP};
                use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};
                let _ = PostMessageW(None, WM_APP, WP(1), LP(0));
            }
        }
        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn default_groups_are_empty_and_in_their_parsed_section() {
        let file: FileConfig =
            toml::from_str(include_str!("../goldendict-auto-search.toml")).expect("default config");
        let gd = file.goldendict.expect("goldendict section");
        assert_eq!(gd.double_click_group.as_deref(), Some(""));
        assert_eq!(gd.clipboard_group.as_deref(), Some(""));
        assert_eq!(gd.clipboard_hotkey_enabled, Some(false));
    }

    #[test]
    fn empty_group_searches_all_dictionaries() {
        assert_eq!(search_args("", false, "word"), vec!["word"]);
        assert_eq!(search_args("", true, "word"), vec!["word"]);
        assert_eq!(
            search_args("translate", false, "word"),
            vec!["--group-name=translate", "word"]
        );
        assert_eq!(
            search_args("translate", true, "word"),
            vec!["--popup-group-name=translate", "word"]
        );
    }

    #[test]
    fn double_click_window_is_300_ms() {
        assert!(is_double_click(1000, 701));
        assert!(!is_double_click(1000, 700));
        assert!(!is_double_click(1000, 0));
    }

    #[test]
    fn clipboard_guard_is_1500_ms() {
        assert!(in_clip_guard(1999, 500));
        assert!(!in_clip_guard(2000, 500));
    }

    #[test]
    fn clipboard_length_cap_is_1000_chars() {
        assert!(!too_long(&"a".repeat(1000)));
        assert!(too_long(&"a".repeat(1001)));
        assert!(!too_long(&"中".repeat(1000)));
        assert!(too_long(&"中".repeat(1001)));
    }

    #[test]
    fn process_binding_toggles_without_touching_others() {
        let mut bound = HashSet::new();
        bound.insert("notepad.exe".into());
        assert!(toggle_binding(&mut bound, "wezterm-gui.exe"));
        assert!(bound.contains("wezterm-gui.exe"));
        assert!(!toggle_binding(&mut bound, "wezterm-gui.exe"));
        assert!(!bound.contains("wezterm-gui.exe"));
        assert!(bound.contains("notepad.exe"));
    }

    #[test]
    fn config_line_replaces_and_inserts_in_the_right_section() {
        let text = "[goldendict]\nexecutable = \"goldendict\"\n\n[settings]\ndouble_click_enabled = false\nclipboard_enabled = false\n";
        let updated = set_key_line(text, "settings", "double_click_enabled", "true");
        assert!(updated.contains("[settings]\ndouble_click_enabled = true"));
        assert!(updated.contains("clipboard_enabled = false"));

        let inserted = set_key_line(
            "[settings]\nclipboard_enabled = false\n",
            "settings",
            "double_click_enabled",
            "true",
        );
        assert!(inserted.contains("[settings]\ndouble_click_enabled = true\nclipboard_enabled"));

        let appended = set_key_line(
            "executable = \"x\"\n",
            "goldendict",
            "clipboard_hotkey_enabled",
            "true",
        );
        assert!(appended.ends_with("[goldendict]\nclipboard_hotkey_enabled = true"));

        let similar = set_key_line(
            "double_click_enabled_extra = 1\n",
            "settings",
            "double_click_enabled",
            "true",
        );
        assert!(similar.contains("double_click_enabled_extra = 1"));
        assert!(similar.ends_with("[settings]\ndouble_click_enabled = true"));
    }
}
