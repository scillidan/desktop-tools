use anyhow::{Context, Result};
use window_core::{Binding, ConfigPart};

pub const DEFAULT_CONFIG: &str = include_str!("../window-quick-switch.toml");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuickSwitchAction {
    FileDialog,
}

impl QuickSwitchAction {
    pub fn as_str(self) -> &'static str {
        "fileDialog"
    }

    fn parse(s: &str) -> Option<Self> {
        (s == "fileDialog").then_some(Self::FileDialog)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QuickSwitchSettings {
    pub enabled: bool,
}

impl Default for QuickSwitchSettings {
    fn default() -> Self {
        Self { enabled: true }
    }
}

pub struct QuickSwitchConfig {
    pub bindings: Vec<Binding<QuickSwitchAction>>,
    pub settings: QuickSwitchSettings,
}

pub fn parse(text: &str) -> Result<QuickSwitchConfig> {
    let ConfigPart { bindings, settings } = window_core::config::parse(
        text,
        QuickSwitchSettings::default(),
        QuickSwitchAction::parse,
        |settings, key, value| {
            if key != "quickSwitch" {
                return Ok(false);
            }
            settings.enabled = value
                .as_bool()
                .context("quickSwitch must be true or false")?;
            Ok(true)
        },
    )?;
    Ok(QuickSwitchConfig { bindings, settings })
}
