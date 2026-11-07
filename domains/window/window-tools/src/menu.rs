//! One Window menu model for hosted IPC and the standalone tray.

use kit::menu::{MenuItem, MenuModel};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmd {
    ToggleAutostart,
    Reload,
    Edit,
    EditResize,
    UnpinAll,
    ToggleSuspend,
    Quit,
}

pub fn model(suspended: bool, autostart: bool, standalone: bool) -> MenuModel {
    let mut items = vec![
        MenuItem::Submenu {
            label: "Rectangle".into(),
            items: vec![
                MenuItem::Item {
                    id: "unpin".into(),
                    label: "Unpin all".into(),
                    enabled: true,
                },
                MenuItem::Item {
                    id: "edit-resize".into(),
                    label: "Edit resize presets".into(),
                    enabled: true,
                },
            ],
        },
        MenuItem::Submenu {
            label: "Window switcher".into(),
            items: vec![MenuItem::Info {
                label: "Cycle windows of the same application".into(),
            }],
        },
        MenuItem::Submenu {
            label: "QuickSwitch".into(),
            items: vec![MenuItem::Info {
                label: "File dialogs follow the file manager".into(),
            }],
        },
        MenuItem::Submenu {
            label: "Virtual desktops".into(),
            items: vec![MenuItem::Info {
                label: "Switch desktops and move windows".into(),
            }],
        },
        MenuItem::Separator,
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
        "unpin" => Cmd::UnpinAll,
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
        assert_eq!(hosted.id_for_label("Unpin all"), Some("unpin"));
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
