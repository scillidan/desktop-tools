//! Native standalone popup for the same menu model sent to the desktop-tools host.

use kit::menu::{MenuItem, MenuModel};
use windows::core::{Result, HSTRING};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, PostMessageW, SetForegroundWindow,
    TrackPopupMenu, HMENU, MF_CHECKED, MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING,
    TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_NONOTIFY, TPM_RETURNCMD, WM_APP, WM_NULL,
};

pub const WM_MENU_COMMAND: u32 = WM_APP + 71;

pub fn command_code(id: &str) -> Option<usize> {
    Some(match id {
        "suspend" => 1,
        "quit" => 2,
        "config-capture" => 4,
        "config-draw" => 5,
        "config-ocr" => 6,
        "config-record" => 7,
        "config-power" => 8,
        "config-flameshot" => 9,
        _ => return None,
    })
}

pub fn command_id(code: usize) -> Option<&'static str> {
    Some(match code {
        1 => "suspend",
        2 => "quit",
        4 => "config-capture",
        5 => "config-draw",
        6 => "config-ocr",
        7 => "config-record",
        8 => "config-power",
        9 => "config-flameshot",
        _ => return None,
    })
}

unsafe fn append_items(
    menu: HMENU,
    items: &[MenuItem],
    next_id: &mut usize,
    commands: &mut Vec<(usize, String)>,
) -> Result<()> {
    for item in items {
        match item {
            MenuItem::Separator => AppendMenuW(menu, MF_SEPARATOR, 0, None)?,
            MenuItem::Info { label } => {
                AppendMenuW(menu, MF_STRING | MF_GRAYED, 0, &HSTRING::from(label))?
            }
            MenuItem::Submenu { label, items } => {
                let submenu = CreatePopupMenu()?;
                append_items(submenu, items, next_id, commands)?;
                AppendMenuW(menu, MF_POPUP, submenu.0 as usize, &HSTRING::from(label))?;
            }
            MenuItem::Item { id, label, enabled }
            | MenuItem::Toggle {
                id, label, enabled, ..
            }
            | MenuItem::Radio {
                id, label, enabled, ..
            } => {
                let checked = match item {
                    MenuItem::Toggle { checked, .. } | MenuItem::Radio { checked, .. } => *checked,
                    _ => false,
                };
                let mut flags = MF_STRING;
                if !enabled {
                    flags |= MF_GRAYED;
                }
                if checked {
                    flags |= MF_CHECKED;
                }
                AppendMenuW(menu, flags, *next_id, &HSTRING::from(label))?;
                commands.push((*next_id, id.clone()));
                *next_id += 1;
            }
        }
    }
    Ok(())
}

/// Shows the native popup above the cursor and returns the selected stable id.
pub unsafe fn show(owner: HWND, model: &MenuModel) -> Option<String> {
    if model.validate().is_err() {
        return None;
    }
    let menu = CreatePopupMenu().ok()?;
    let mut commands = Vec::new();
    let mut next_id = 1;
    if append_items(menu, &model.items, &mut next_id, &mut commands).is_err() {
        let _ = DestroyMenu(menu);
        return None;
    }
    let mut pt = POINT::default();
    if GetCursorPos(&mut pt).is_err() {
        let _ = DestroyMenu(menu);
        return None;
    }

    let _ = SetForegroundWindow(owner);
    let selected = TrackPopupMenu(
        menu,
        TPM_BOTTOMALIGN | TPM_LEFTALIGN | TPM_RETURNCMD | TPM_NONOTIFY,
        pt.x,
        pt.y,
        None,
        owner,
        None,
    );
    // Dismissed native menus otherwise leave the owner in a modal state.
    let _ = PostMessageW(Some(owner), WM_NULL, WPARAM(0), LPARAM(0));
    let _ = DestroyMenu(menu);
    commands
        .into_iter()
        .find(|(id, _)| *id == selected.0 as usize)
        .map(|(_, command)| command)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_files;

    #[test]
    fn native_menus_are_alphabetical_and_external_handoff_is_last() {
        let model = crate::screen_menu(false, true);
        model.validate().unwrap();
        let labels: Vec<&str> = model
            .items
            .iter()
            .filter_map(|item| match item {
                MenuItem::Submenu { label, .. } => Some(label.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            labels,
            [
                "Capture",
                "Draw",
                "OCR",
                "Power",
                "Record",
                "Flameshot Handoff"
            ]
        );
        for module in config_files::MODULES {
            assert!(model
                .id_for_label(&format!("Edit {} config", module.label()))
                .is_some());
        }
        assert!(model.id_for_label("Suspend Group \"Screen\"").is_some());
        assert!(model.id_for_label("Exit Screen Tools").is_some());
    }

    #[test]
    fn hosted_menu_leaves_process_exit_to_the_host() {
        let model = crate::screen_menu(false, false);
        assert!(model.id_for_label("Exit Screen Tools").is_none());
        assert!(model.id_for_label("Edit Screen actions config").is_none());
    }
}
