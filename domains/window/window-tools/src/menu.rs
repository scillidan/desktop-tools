//! One Window menu model for hosted IPC and the standalone tray.

use kit::menu::{MenuItem, MenuModel};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmd {
    ToggleAutostart,
    Reload,
    Edit,
    EditResize,
    ClearAllPins,
    Undo,
    ToggleSuspend,
    Quit,
}

fn info(label: &str, description: &str) -> MenuItem {
    MenuItem::Submenu {
        label: label.into(),
        items: vec![MenuItem::Info {
            label: description.into(),
        }],
    }
}

pub fn model(suspended: bool, autostart: bool, standalone: bool) -> MenuModel {
    let mut items = vec![
        info("Rectangle", "Snap windows to rectangles"),
        MenuItem::Submenu {
            label: "Resize".into(),
            items: vec![
                MenuItem::Info {
                    label: "Sizer and resize presets".into(),
                },
                MenuItem::Item {
                    id: "edit-resize".into(),
                    label: "Edit resize presets".into(),
                    enabled: true,
                },
            ],
        },
        info("Mouse Drag", "Drag windows with a modifier and left button"),
        MenuItem::Submenu {
            label: "On Top/Bottom".into(),
            items: vec![MenuItem::Item {
                id: "clear-pins".into(),
                label: "Clear all pins".into(),
                enabled: true,
            }],
        },
        info(
            "Maximize/Minimize",
            "Maximize, restore, or minimize windows",
        ),
        info("App Window Switch", "Cycle windows of the same application"),
        info("Quick Switch", "File dialogs follow the file manager"),
        info("Virtual Desktops", "Switch desktops and move windows"),
        MenuItem::Separator,
        MenuItem::Item {
            id: "undo".into(),
            label: "Undo Window Action".into(),
            enabled: true,
        },
        MenuItem::Toggle {
            id: "suspend".into(),
            label: "Suspend all hotkeys".into(),
            checked: suspended,
            enabled: true,
        },
        MenuItem::Item {
            id: "reload".into(),
            label: "Reload window-tools.toml".into(),
            enabled: true,
        },
    ];
    if standalone {
        items.push(MenuItem::Item {
            id: "edit".into(),
            label: "Edit window-tools.toml".into(),
            enabled: true,
        });
        items.push(MenuItem::Toggle {
            id: "autostart".into(),
            label: "Start with Windows".into(),
            checked: autostart,
            enabled: true,
        });
        items.push(MenuItem::Item {
            id: "quit".into(),
            label: "Exit window-tools".into(),
            enabled: true,
        });
    }
    MenuModel::new(items)
}

pub fn command(id: &str) -> Option<Cmd> {
    Some(match id {
        "autostart" => Cmd::ToggleAutostart,
        "reload" => Cmd::Reload,
        "edit" => Cmd::Edit,
        "edit-resize" => Cmd::EditResize,
        "clear-pins" => Cmd::ClearAllPins,
        "undo" => Cmd::Undo,
        "suspend" => Cmd::ToggleSuspend,
        "quit" => Cmd::Quit,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosted_and_standalone_share_valid_module_models() {
        let hosted = model(true, false, false);
        hosted.validate().unwrap();
        assert_eq!(hosted.id_for_label("Clear all pins"), Some("clear-pins"));
        assert_eq!(hosted.id_for_label("Undo Window Action"), Some("undo"));
        assert_eq!(hosted.id_for_label("Exit window-tools"), None);
        assert_eq!(hosted.id_for_label("Edit window-tools.toml"), None);

        let standalone = model(false, true, true);
        standalone.validate().unwrap();
        assert_eq!(
            standalone.id_for_label("Start with Windows"),
            Some("autostart")
        );
        assert_eq!(command("quit"), Some(Cmd::Quit));
        assert_eq!(command("unknown"), None);
    }
}
