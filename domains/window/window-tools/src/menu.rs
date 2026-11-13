//! One Window menu model for hosted IPC and the standalone tray.

use crate::config::ConfigModule;
use kit::menu::{MenuItem, MenuModel};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmd {
    ToggleAutostart,
    Reload,
    Edit,
    EditResize,
    EditModule(ConfigModule),
    ClearAllPins,
    Undo,
    ToggleSuspend,
    Quit,
}

fn module(label: &str, description: &str, id: &str, edit: &str) -> MenuItem {
    MenuItem::Submenu {
        label: label.into(),
        items: vec![
            MenuItem::Info {
                label: description.into(),
            },
            MenuItem::Separator,
            MenuItem::Item {
                id: id.into(),
                label: edit.into(),
                enabled: true,
            },
        ],
    }
}

pub fn model(suspended: bool, autostart: bool, standalone: bool) -> MenuModel {
    let mut items = vec![
        module(
            "Rectangle",
            "Snap windows to rectangles",
            "edit-rectangle",
            "Edit Rectangle config",
        ),
        MenuItem::Submenu {
            label: "Resize".into(),
            items: vec![
                MenuItem::Info {
                    label: "Sizer presets and fullscreen window zoom".into(),
                },
                MenuItem::Separator,
                MenuItem::Item {
                    id: "edit-resize".into(),
                    label: "Edit Resize config".into(),
                    enabled: true,
                },
                MenuItem::Item {
                    id: "edit-scale".into(),
                    label: "Edit Scale config".into(),
                    enabled: true,
                },
                MenuItem::Item {
                    id: "edit-resize-presets".into(),
                    label: "Edit resize presets".into(),
                    enabled: true,
                },
            ],
        },
        module(
            "Mouse Drag",
            "Drag windows with a modifier and left button",
            "edit-mouse-drag",
            "Edit Mouse Drag config",
        ),
        MenuItem::Submenu {
            label: "On Top/Bottom".into(),
            items: vec![
                MenuItem::Info {
                    label: "Topmost and bottommost window pins".into(),
                },
                MenuItem::Separator,
                MenuItem::Item {
                    id: "clear-pins".into(),
                    label: "Clear all pins".into(),
                    enabled: true,
                },
                MenuItem::Item {
                    id: "edit-on-top-bottom".into(),
                    label: "Edit On Top/Bottom config".into(),
                    enabled: true,
                },
            ],
        },
        module(
            "Maximize/Minimize",
            "Maximize, restore, or minimize windows",
            "edit-maximize-minimize",
            "Edit Maximize/Minimize config",
        ),
        module(
            "App Window Switch",
            "Cycle windows of the same application",
            "edit-app-switch",
            "Edit App Window Switch config",
        ),
        module(
            "Quick Switch",
            "File dialogs follow the file manager",
            "edit-quick-switch",
            "Edit Quick Switch config",
        ),
        module(
            "Virtual Desktops",
            "Switch desktops and move windows",
            "edit-virtual-desktops",
            "Edit Virtual Desktops config",
        ),
        MenuItem::Separator,
        MenuItem::Item {
            id: "undo".into(),
            label: "Undo Window Action".into(),
            enabled: true,
        },
        MenuItem::Toggle {
            id: "suspend".into(),
            label: "Suspend Group \"Window\"".into(),
            checked: suspended,
            enabled: true,
        },
    ];
    let feature_end = items
        .iter()
        .position(|item| matches!(item, MenuItem::Separator))
        .unwrap_or(items.len());
    items[..feature_end].sort_by_key(|item| match item {
        MenuItem::Submenu { label, .. } => label.to_lowercase(),
        _ => String::new(),
    });
    items.push(MenuItem::Item {
        id: "edit".into(),
        label: "Edit Window actions config".into(),
        enabled: true,
    });
    if standalone {
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
        "edit-resize-presets" => Cmd::EditResize,
        "edit-rectangle" => Cmd::EditModule(ConfigModule::Rectangle),
        "edit-resize" => Cmd::EditModule(ConfigModule::Resize),
        "edit-scale" => Cmd::EditModule(ConfigModule::Scale),
        "edit-mouse-drag" => Cmd::EditModule(ConfigModule::MouseDrag),
        "edit-on-top-bottom" => Cmd::EditModule(ConfigModule::OnTopBottom),
        "edit-maximize-minimize" => Cmd::EditModule(ConfigModule::MaximizeMinimize),
        "edit-app-switch" => Cmd::EditModule(ConfigModule::AppSwitch),
        "edit-quick-switch" => Cmd::EditModule(ConfigModule::QuickSwitch),
        "edit-virtual-desktops" => Cmd::EditModule(ConfigModule::VirtualDesktops),
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
        assert_eq!(
            hosted.id_for_label("Suspend Group \"Window\""),
            Some("suspend")
        );
        assert_eq!(hosted.id_for_label("Reload window configs"), None);
        assert_eq!(hosted.id_for_label("Exit window-tools"), None);
        assert_eq!(hosted.id_for_label("Edit window-tools.toml"), None);
        assert_eq!(
            hosted.id_for_label("Edit Window actions config"),
            Some("edit")
        );
        assert_eq!(
            hosted.id_for_label("Edit Rectangle config"),
            Some("edit-rectangle")
        );
        let feature_names: Vec<&str> = hosted.items[..8]
            .iter()
            .map(|item| match item {
                MenuItem::Submenu { label, .. } => label.as_str(),
                _ => panic!("feature must be a submenu"),
            })
            .collect();
        assert_eq!(
            feature_names,
            [
                "App Window Switch",
                "Maximize/Minimize",
                "Mouse Drag",
                "On Top/Bottom",
                "Quick Switch",
                "Rectangle",
                "Resize",
                "Virtual Desktops"
            ]
        );

        for item in &hosted.items[..8] {
            let MenuItem::Submenu { items, .. } = item else {
                panic!("feature must be a submenu")
            };
            assert!(matches!(items.first(), Some(MenuItem::Info { .. })));
            assert!(matches!(items.get(1), Some(MenuItem::Separator)));
        }

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
