use anyhow::Result;
use window_core::{Binding, ConfigPart};

pub const DEFAULT_CONFIG: &str = include_str!("../window-app-switch.toml");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppSwitchAction {
    NextWindow,
    PrevWindow,
}

impl AppSwitchAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NextWindow => "nextWindow",
            Self::PrevWindow => "prevWindow",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "nextWindow" => Self::NextWindow,
            "prevWindow" => Self::PrevWindow,
            _ => return None,
        })
    }
}

#[derive(Default)]
pub struct AppSwitchConfig {
    pub bindings: Vec<Binding<AppSwitchAction>>,
}

pub fn parse(text: &str) -> Result<AppSwitchConfig> {
    let ConfigPart { bindings, .. } =
        window_core::config::parse(text, (), AppSwitchAction::parse, |_, _, _| Ok(false))?;
    Ok(AppSwitchConfig { bindings })
}
