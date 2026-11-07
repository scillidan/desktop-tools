//! Plugin menu model shared by the standalone tray and the host IPC.
//!
//! IDs are stable action names; labels are display-only. The host namespaces
//! IDs by plugin when routing, so two plugins may both use `toggle`.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const MAX_ITEMS: usize = 32;
pub const MAX_LABEL: usize = 120;
pub const MAX_ID: usize = 64;
pub const MAX_DEPTH: usize = 4;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct MenuModel {
    pub items: Vec<MenuItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum MenuItem {
    Item {
        id: String,
        label: String,
        #[serde(default = "yes")]
        enabled: bool,
    },
    Toggle {
        id: String,
        label: String,
        checked: bool,
        #[serde(default = "yes")]
        enabled: bool,
    },
    Radio {
        id: String,
        label: String,
        checked: bool,
        #[serde(default = "yes")]
        enabled: bool,
    },
    Info {
        label: String,
    },
    Submenu {
        label: String,
        items: Vec<MenuItem>,
    },
    Separator,
}

fn yes() -> bool {
    true
}

impl MenuModel {
    pub fn new(items: Vec<MenuItem>) -> Self {
        Self { items }
    }

    pub fn validate(&self) -> Result<()> {
        let mut count = 0;
        let mut ids = HashSet::new();
        let mut labels = HashSet::new();
        validate_items(&self.items, 1, &mut count, &mut ids, &mut labels)
    }

    /// Resolve a standalone tray label back to its stable id. Interactive
    /// labels are unique across the complete tree, so no positional state is
    /// needed.
    pub fn id_for_label(&self, label: &str) -> Option<&str> {
        id_for_label(&self.items, label)
    }

    /// Render the same model into the standalone tray. Disabled interactive
    /// rows become grey info rows.
    #[cfg(windows)]
    pub fn render(&self) -> Vec<crate::tray::Entry> {
        render_items(&self.items)
    }
}

fn validate_items(
    items: &[MenuItem],
    depth: usize,
    count: &mut usize,
    ids: &mut HashSet<String>,
    labels: &mut HashSet<String>,
) -> Result<()> {
    if depth > MAX_DEPTH {
        bail!("menu nesting exceeds {MAX_DEPTH}");
    }
    *count += items.len();
    if *count > MAX_ITEMS {
        bail!("menu has more than {MAX_ITEMS} items");
    }
    for item in items {
        match item {
            MenuItem::Separator => {}
            MenuItem::Info { label } => check_label(label)?,
            MenuItem::Submenu { label, items } => {
                check_label(label)?;
                if !labels.insert(label.clone()) {
                    bail!("duplicate menu label `{label}`");
                }
                validate_items(items, depth + 1, count, ids, labels)?;
            }
            MenuItem::Item {
                id,
                label,
                enabled: _,
            }
            | MenuItem::Toggle {
                id,
                label,
                checked: _,
                enabled: _,
            }
            | MenuItem::Radio {
                id,
                label,
                checked: _,
                enabled: _,
            } => {
                check_id(id)?;
                check_label(label)?;
                if !ids.insert(id.clone()) {
                    bail!("duplicate menu id `{id}`");
                }
                if !labels.insert(label.clone()) {
                    bail!("duplicate menu label `{label}`");
                }
            }
        }
    }
    Ok(())
}

fn id_for_label<'a>(items: &'a [MenuItem], label: &str) -> Option<&'a str> {
    for item in items {
        match item {
            MenuItem::Item { id, label: l, .. }
            | MenuItem::Toggle { id, label: l, .. }
            | MenuItem::Radio { id, label: l, .. }
                if l == label =>
            {
                return Some(id);
            }
            MenuItem::Submenu { items, .. } => {
                if let Some(id) = id_for_label(items, label) {
                    return Some(id);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(windows)]
fn render_items(items: &[MenuItem]) -> Vec<crate::tray::Entry> {
    use crate::tray::Entry;
    items
        .iter()
        .map(|item| match item {
            MenuItem::Separator => Entry::Sep,
            MenuItem::Info { label } => Entry::Info(label.clone()),
            MenuItem::Submenu { label, items } => {
                Entry::SubmenuOwned(label.clone(), render_items(items))
            }
            MenuItem::Item {
                id,
                label,
                enabled: true,
            } => Entry::ItemId(id.clone(), label.clone()),
            MenuItem::Item {
                label,
                enabled: false,
                ..
            } => Entry::Info(label.clone()),
            MenuItem::Toggle {
                id,
                label,
                checked,
                enabled: true,
            }
            | MenuItem::Radio {
                id,
                label,
                checked,
                enabled: true,
            } => Entry::ToggleId(id.clone(), label.clone(), *checked),
            MenuItem::Toggle { label, .. } | MenuItem::Radio { label, .. } => {
                Entry::Info(label.clone())
            }
        })
        .collect()
}

fn check_id(id: &str) -> Result<()> {
    if id.is_empty() || id.len() > MAX_ID || id.contains(['\n', '\r', '\t']) {
        bail!("invalid menu id `{id}`");
    }
    Ok(())
}

fn check_label(label: &str) -> Result<()> {
    if label.is_empty() || label.chars().count() > MAX_LABEL || label.contains(['\n', '\r']) {
        bail!("invalid menu label `{label}`");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> MenuModel {
        MenuModel::new(vec![
            MenuItem::Info {
                label: "Status: locked".into(),
            },
            MenuItem::Toggle {
                id: "lock".into(),
                label: "Lock keyboard".into(),
                checked: true,
                enabled: true,
            },
            MenuItem::Item {
                id: "reload".into(),
                label: "Reload".into(),
                enabled: true,
            },
            MenuItem::Separator,
        ])
    }

    #[test]
    fn json_round_trip_is_an_array() {
        let model = sample();
        let json = serde_json::to_string(&model).unwrap();
        assert!(json.starts_with('['));
        let again: MenuModel = serde_json::from_str(&json).unwrap();
        assert_eq!(model, again);
        assert_eq!(model.id_for_label("Lock keyboard"), Some("lock"));
        assert_eq!(model.id_for_label("Status: locked"), None);
    }

    #[test]
    fn duplicate_ids_and_long_labels_are_rejected() {
        let mut model = sample();
        model.items.push(MenuItem::Item {
            id: "lock".into(),
            label: "Other".into(),
            enabled: true,
        });
        assert!(model.validate().is_err());

        let model = MenuModel::new(vec![MenuItem::Info {
            label: "x".repeat(MAX_LABEL + 1),
        }]);
        assert!(model.validate().is_err());
    }

    #[test]
    fn nested_submenus_validate_route_and_render() {
        let model = MenuModel::new(vec![MenuItem::Submenu {
            label: "Rectangle".into(),
            items: vec![MenuItem::Item {
                id: "unpin".into(),
                label: "Unpin all".into(),
                enabled: true,
            }],
        }]);
        model.validate().unwrap();
        assert_eq!(model.id_for_label("Unpin all"), Some("unpin"));
        let json = serde_json::to_string(&model).unwrap();
        assert_eq!(serde_json::from_str::<MenuModel>(&json).unwrap(), model);
        #[cfg(windows)]
        assert!(matches!(
            model.render().as_slice(),
            [crate::tray::Entry::SubmenuOwned(label, _)] if label == "Rectangle"
        ));

        let duplicate = MenuModel::new(vec![
            model.items[0].clone(),
            MenuItem::Item {
                id: "unpin".into(),
                label: "Other".into(),
                enabled: true,
            },
        ]);
        assert!(duplicate.validate().is_err());
    }

    #[test]
    fn disabled_rows_render_grey() {
        let model = MenuModel::new(vec![MenuItem::Item {
            id: "x".into(),
            label: "X".into(),
            enabled: false,
        }]);
        #[cfg(windows)]
        {
            let rendered = model.render();
            assert!(matches!(rendered[0], crate::tray::Entry::Info(_)));
        }
    }
}
