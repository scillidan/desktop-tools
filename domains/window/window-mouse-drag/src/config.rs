use anyhow::{Context, Result};
use window_core::Mods;

pub const DEFAULT_CONFIG: &str = include_str!("../window-mouse-drag.toml");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MouseDragSettings {
    pub drag_modifier: Mods,
}

impl Default for MouseDragSettings {
    fn default() -> Self {
        Self {
            drag_modifier: Mods {
                ctrl: false,
                shift: false,
                alt: true,
                win: true,
            },
        }
    }
}

pub struct MouseDragConfig {
    pub settings: MouseDragSettings,
}

pub fn parse(text: &str) -> Result<MouseDragConfig> {
    let part = window_core::config::parse(
        text,
        MouseDragSettings::default(),
        |_| None::<()>,
        |settings, key, value| {
            if key != "dragModifier" {
                return Ok(false);
            }
            let raw = value.as_str().context("dragModifier must be a string")?;
            settings.drag_modifier = raw.parse().context("invalid dragModifier")?;
            Ok(true)
        },
    )?;
    Ok(MouseDragConfig {
        settings: part.settings,
    })
}
