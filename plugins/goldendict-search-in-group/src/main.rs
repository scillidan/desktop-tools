#![cfg_attr(windows, windows_subsystem = "windows")]

//! goldendict-search-in-group: press a per-group hotkey to search the current
//! clipboard text in GoldenDict restricted to that dictionary group.
//! Ported from AHK-GoldenDictSearchInGroup (INI -> TOML, same behavior).
//!
//! Parity notes:
//! - The AHK original saves/restores `ClipboardAll` but never writes the
//!   clipboard, so the restore is a no-op; we just read the text.
//! - AHK hotkey syntax (`!+z`, `^!+g`) parses via kit's combo parser.
//! - Double-press (`!xx` = press `!x` twice within 400 ms) is detected with
//!   the same `A_TickCount`-style timestamp map.

fn main() -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        if let Err(e) = run() {
            kit::win::fatal_error("goldendict-search-in-group", &format!("Error:\n{e:#}"));
        }
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        kit::unimplemented("goldendict-search-in-group")
    }
}

/// (base hotkey, is_double_press): `!xx` -> (`!x`, true).
///
/// AHK's textual rule is "last two characters identical", but that also turns
/// `f11` into double-F1. Parse the whole combo first: if it is a valid single
/// hotkey, it wins. Otherwise try the AHK double-press split and keep it only
/// if the base is a valid combo. This supports both `!xx` and `alt+shift+zz`.
#[cfg(windows)]
fn split_double_press(hotkey: &str) -> (&str, bool) {
    let hotkey = hotkey.trim();
    if hotkey.parse::<kit::keys::Combo>().is_ok() {
        return (hotkey, false);
    }
    let chars: Vec<char> = hotkey.chars().collect();
    if chars.len() >= 2 && chars[chars.len() - 1] == chars[chars.len() - 2] {
        let base_len = hotkey.len() - chars[chars.len() - 1].len_utf8();
        let base = &hotkey[..base_len];
        if base.parse::<kit::keys::Combo>().is_ok() {
            return (base, true);
        }
    }
    (hotkey, false)
}

/// AHK `Loop, 10` + `if (groupName != "")`: at most ten slots, empty names
/// ignored. A group without a hotkey is still kept as a config entry.
#[cfg(windows)]
fn load_groups(mut groups: Vec<Group>) -> Vec<Group> {
    groups.truncate(10);
    groups.into_iter().filter(|g| !g.name.is_empty()).collect()
}

/// GoldenDict CLI group selector.
#[cfg(windows)]
fn group_parameter(group: &str, popup: bool) -> String {
    if popup {
        format!("--popup-group-name={group}")
    } else {
        format!("--group-name={group}")
    }
}

/// Replace `clipboard_hotkey_enabled` in the current file text, inserting it
/// under `[goldendict]` if the user removed the line.
#[cfg(windows)]
fn set_popup_line(text: &str, popup: bool) -> String {
    let new_line = format!("clipboard_hotkey_enabled = {popup}");
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let mut replaced = false;
    for line in &mut lines {
        let key = line.trim_start().split('=').next().unwrap_or("").trim();
        if key == "clipboard_hotkey_enabled" {
            *line = new_line.clone();
            replaced = true;
            break;
        }
    }
    if !replaced {
        if let Some(pos) = lines.iter().position(|l| l.trim() == "[goldendict]") {
            lines.insert(pos + 1, new_line);
        } else {
            lines.push("[goldendict]".into());
            lines.push(new_line);
        }
    }
    lines.join("\n")
}

#[cfg(windows)]
#[derive(Clone, serde::Deserialize)]
struct Group {
    name: String,
    hotkey: Option<String>,
}

#[cfg(windows)]
#[derive(serde::Deserialize)]
struct FileConfig {
    goldendict: Option<GdSection>,
    messages: Option<Messages>,
    group: Option<Vec<Group>>,
}

#[cfg(windows)]
#[derive(serde::Deserialize)]
struct GdSection {
    executable: Option<String>,
    clipboard_hotkey_enabled: Option<bool>,
}

#[cfg(windows)]
#[derive(serde::Deserialize)]
struct Messages {
    no_selection: Option<String>,
}

#[cfg(windows)]
const DBL_CLICK_MS: u128 = 400; // AHK DblClickTime

#[cfg(windows)]
fn run() -> anyhow::Result<()> {
    use kit::keys::Combo;
    use kit::tray::{Event, Tray};
    use kit::{config, hotkey::Hotkeys, win};
    use std::collections::HashMap;
    use std::time::Instant;

    const APP: &str = "goldendict-search-in-group";
    let hosted = std::env::args().any(|a| a == "--plugin");
    let no_tray = hosted && std::env::args().any(|a| a == "--no-tray");

    let _single = win::ensure_single_instance(APP)?;

    let (config_path, text) = config::load_profile(
        "goldendict-search-in-group.toml",
        include_str!("../goldendict-search-in-group.toml"),
    )?;
    let file: FileConfig = toml::from_str(&text)?;
    let gd = file.goldendict.unwrap_or(GdSection {
        executable: None,
        clipboard_hotkey_enabled: None,
    });
    let no_selection = file
        .messages
        .and_then(|m| m.no_selection)
        .unwrap_or_else(|| "No text copied. Please select the word to query first.".into());

    let groups = load_groups(file.group.unwrap_or_default());

    // Hotkeys: one per group. Double-press groups register their base combo
    // (AHK: `Hotkey, %baseHk%, HandleDoublePress`).
    let mut hotkey_defs: Vec<(String, Combo, bool)> = Vec::new(); // (label, combo, double_press)
    let mut tooltip = String::from("GoldenDict Search\nGroups and hotkeys");
    for (i, g) in groups.iter().enumerate() {
        if let Some(hk) = &g.hotkey {
            let hk = hk.trim();
            if hk.is_empty() {
                continue;
            }
            let (base, dbl) = split_double_press(hk);
            let combo: Combo = base.parse()?;
            tooltip.push_str(&format!("\n  {} = {}", g.name, hk));
            hotkey_defs.push((i.to_string(), combo, dbl));
        }
    }
    if hotkey_defs.is_empty() {
        tooltip.push_str("\n  (no group hotkeys configured)");
    }

    let hotkeys = Hotkeys::register(
        &hotkey_defs
            .iter()
            .map(|(l, c, _)| (l.as_str(), *c))
            .collect::<Vec<_>>(),
    )?;

    let mut popup = gd.clipboard_hotkey_enabled.unwrap_or(false);
    let exe = gd.executable.unwrap_or_else(|| "goldendict".into());
    let mut last_press: HashMap<String, Instant> = HashMap::new();

    if no_tray {
        use kit::ipc::{HostCommand, PluginLink};
        let link = PluginLink::new()?;
        link.ready(menu_model(popup), Some(status_text(popup)))?;
        kit::tray::run_headless(Some(&hotkeys), |event| match event {
            Event::Hotkey(label) => {
                let Ok(i) = label.parse::<usize>() else {
                    return;
                };
                let Some((_, _, dbl)) = hotkey_defs.get(i) else {
                    return;
                };
                if *dbl {
                    let now = Instant::now();
                    let hit = last_press
                        .get(&label)
                        .is_some_and(|t| now.duration_since(*t).as_millis() < DBL_CLICK_MS);
                    if hit {
                        last_press.remove(&label);
                        search_with_notify(
                            &exe,
                            &groups[i].name,
                            popup,
                            &no_selection,
                            Some(&link),
                        );
                    } else {
                        last_press.insert(label, now);
                    }
                } else {
                    search_with_notify(&exe, &groups[i].name, popup, &no_selection, Some(&link));
                }
            }
            Event::Wakeup(4) => {
                while let Some(command) = link.try_recv() {
                    match command {
                        HostCommand::Invoke { id, .. } if id == "popup" => {
                            popup = !popup;
                            let current = std::fs::read_to_string(&config_path)
                                .unwrap_or_else(|_| text.clone());
                            let updated = set_popup_line(&current, popup);
                            if let Err(e) = std::fs::write(&config_path, updated) {
                                eprintln!("cannot save config: {e:#}");
                                let _ = link.notify(APP, format!("Cannot save config:\n{e:#}"));
                            }
                            let _ = link.menu(&menu_model(popup));
                            let _ = link.status(&status_text(popup));
                        }
                        HostCommand::Quit { .. } => kit::tray::quit(),
                        _ => {}
                    }
                }
            }
            _ => {}
        });
        return Ok(());
    }

    let mut tray = Tray::new(
        &tooltip,
        kit::icon::letter_rgba('g', [0xb8, 0x86, 0x0b, 0xff], [0x7c, 0x59, 0x06, 0xff]),
    )?;
    refresh_menu(&mut tray, popup, hosted);

    tray.run(Some(&hotkeys), |tray, event| match event {
        Event::Hotkey(label) => {
            let Ok(i) = label.parse::<usize>() else {
                return;
            };
            let Some((_, _, dbl)) = hotkey_defs.get(i) else {
                return;
            };
            if *dbl {
                let now = Instant::now();
                let hit = last_press
                    .get(&label)
                    .is_some_and(|t| now.duration_since(*t).as_millis() < DBL_CLICK_MS);
                if hit {
                    last_press.remove(&label);
                    search(&exe, &groups[i].name, popup, &no_selection);
                } else {
                    last_press.insert(label, now);
                }
            } else {
                search(&exe, &groups[i].name, popup, &no_selection);
            }
        }
        Event::MenuId(id) if id == "popup" => {
            popup = !popup;
            // Persist from the current file, not the startup snapshot, so
            // edits made since launch are not overwritten.
            let current = std::fs::read_to_string(&config_path).unwrap_or_else(|_| text.clone());
            let updated = set_popup_line(&current, popup);
            if let Err(e) = std::fs::write(&config_path, updated) {
                win::message_box(APP, &format!("Cannot save config:\n{e:#}"));
            }
            refresh_menu(tray, popup, hosted);
        }
        Event::Menu(label) if label == "Start with Windows" => {
            let now = !win::autostart_enabled(APP);
            if let Err(e) = win::set_autostart(APP, now) {
                win::message_box(APP, &format!("Autostart failed:\n{e:#}"));
            }
            refresh_menu(tray, popup, hosted);
        }
        Event::Menu(label) if label == "Edit Config" => {
            win::edit_config(&config_path);
        }
        Event::Menu(label) if label == "Reload" => {
            config::restart_self();
            tray.quit();
        }
        Event::Menu(label) if label == "Exit" => tray.quit(),
        _ => {}
    });
    Ok(())
}

#[cfg(windows)]
fn menu_model(popup: bool) -> kit::menu::MenuModel {
    use kit::menu::{MenuItem, MenuModel};
    MenuModel::new(vec![MenuItem::Toggle {
        id: "popup".into(),
        label: "Clipboard Hotkey (Popup Search)".into(),
        checked: popup,
        enabled: true,
    }])
}

#[cfg(windows)]
fn status_text(popup: bool) -> String {
    format!("popup={}", if popup { "on" } else { "off" })
}

/// Look up the clipboard text in GoldenDict restricted to `group`.
#[cfg(windows)]
fn search(exe: &str, group: &str, popup: bool, no_selection: &str) {
    search_with_notify(exe, group, popup, no_selection, None);
}

#[cfg(windows)]
fn search_with_notify(
    exe: &str,
    group: &str,
    popup: bool,
    no_selection: &str,
    link: Option<&kit::ipc::PluginLink>,
) {
    let report = |message: &str| {
        if let Some(link) = link {
            let _ = link.notify("goldendict-search-in-group", message);
        } else {
            kit::win::message_box("goldendict-search-in-group", message);
        }
    };
    let query = arboard::Clipboard::new()
        .ok()
        .and_then(|mut cb| cb.get_text().ok())
        .unwrap_or_default()
        .trim()
        .to_string();
    if query.is_empty() {
        report(no_selection);
        return;
    }
    let param = group_parameter(group, popup);
    if let Err(e) = std::process::Command::new(exe)
        .arg(param)
        .arg(query)
        .spawn()
    {
        report(&format!(
            "Cannot start GoldenDict:\n{e:#}\n\nExecutable: {exe}"
        ));
    }
}

#[cfg(windows)]
fn refresh_menu(tray: &mut kit::tray::Tray, popup: bool, hosted: bool) {
    use kit::tray::Entry;
    let mut entries = menu_model(popup).render();
    if !hosted {
        entries.push(Entry::Toggle(
            "Start with Windows",
            kit::win::autostart_enabled("goldendict-search-in-group"),
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

#[cfg(test)]
mod tests {
    use super::{group_parameter, load_groups, set_popup_line, split_double_press, Group};

    #[test]
    fn double_press_detection() {
        assert_eq!(split_double_press("!xx"), ("!x", true));
        assert_eq!(split_double_press("xx"), ("x", true));
        assert_eq!(split_double_press("alt+shift+zz"), ("alt+shift+z", true));
        assert_eq!(split_double_press("!11"), ("!1", true));
        assert_eq!(split_double_press("alt+1"), ("alt+1", false));
        assert_eq!(split_double_press("!+z"), ("!+z", false));
        assert_eq!(split_double_press("x"), ("x", false));
    }

    #[test]
    fn function_keys_are_not_double_press() {
        // AHK's textual rule would split this into double-F1; a valid combo
        // must win.
        assert_eq!(split_double_press("f11"), ("f11", false));
        assert_eq!(
            split_double_press("ctrl+alt+shift+insert"),
            ("ctrl+alt+shift+insert", false)
        );
    }

    #[test]
    fn groups_are_capped_at_ten_and_skip_empty_names() {
        let mut groups: Vec<Group> = (0..12)
            .map(|i| Group {
                name: if i == 0 {
                    String::new()
                } else {
                    format!("g{i}")
                },
                hotkey: None,
            })
            .collect();
        groups.push(Group {
            name: "after-cap".into(),
            hotkey: None,
        });
        let loaded = load_groups(groups);
        assert_eq!(loaded.len(), 9); // 10 slots, first empty
        assert_eq!(loaded[0].name, "g1");
        assert_eq!(loaded.last().unwrap().name, "g9"); // g10 is slot 11
    }

    #[test]
    fn group_parameter_switches_popup_flag() {
        assert_eq!(group_parameter("default", false), "--group-name=default");
        assert_eq!(
            group_parameter("我的 组", true),
            "--popup-group-name=我的 组"
        );
    }

    #[test]
    fn popup_line_replaces_and_inserts() {
        let text = "[goldendict]\nexecutable = \"goldendict\"\nclipboard_hotkey_enabled = false\n";
        let updated = set_popup_line(text, true);
        assert!(updated.contains("clipboard_hotkey_enabled = true"));
        assert!(!updated.contains("clipboard_hotkey_enabled = false"));

        let inserted = set_popup_line("[goldendict]\nexecutable = \"x\"\n", true);
        assert!(inserted.contains("[goldendict]\nclipboard_hotkey_enabled = true\nexecutable"));

        let appended = set_popup_line("executable = \"x\"\n", false);
        assert!(appended.ends_with("[goldendict]\nclipboard_hotkey_enabled = false"));
    }
}
