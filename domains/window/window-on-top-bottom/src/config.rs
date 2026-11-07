use crate::OnTopBottomAction;
use anyhow::{Context, Result};
use window_core::{Binding, ConfigPart};

pub const DEFAULT_CONFIG: &str = include_str!("../window-on-top-bottom.toml");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OnTopBottomSettings {
    pub bottom_reassert_ms: u64,
}

impl Default for OnTopBottomSettings {
    fn default() -> Self {
        Self {
            bottom_reassert_ms: 250,
        }
    }
}

pub struct OnTopBottomConfig {
    pub bindings: Vec<Binding<OnTopBottomAction>>,
    pub settings: OnTopBottomSettings,
}

impl OnTopBottomAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ToggleAlwaysOnTop => "toggleAlwaysOnTop",
            Self::ToggleAlwaysOnBottom => "toggleAlwaysOnBottom",
            Self::ClearAllPins => "unpinAll",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "toggleAlwaysOnTop" => Self::ToggleAlwaysOnTop,
            "toggleAlwaysOnBottom" => Self::ToggleAlwaysOnBottom,
            "unpinAll" => Self::ClearAllPins,
            _ => return None,
        })
    }
}

pub fn parse(text: &str) -> Result<OnTopBottomConfig> {
    let ConfigPart { bindings, settings } = window_core::config::parse(
        text,
        OnTopBottomSettings::default(),
        OnTopBottomAction::parse,
        |settings, key, value| {
            if key != "bottomReassertMs" {
                return Ok(false);
            }
            settings.bottom_reassert_ms = value
                .as_integer()
                .context("bottomReassertMs must be an integer")?
                .clamp(100, 60_000) as u64;
            Ok(true)
        },
    )?;
    Ok(OnTopBottomConfig { bindings, settings })
}
