use crate::ResizeAction;
use anyhow::Result;
use window_core::{Binding, ConfigPart};

pub const DEFAULT_CONFIG: &str = include_str!("../window-resize.toml");

#[derive(Default)]
pub struct ResizeConfig {
    pub bindings: Vec<Binding<ResizeAction>>,
}

impl ResizeAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SizerMenu => "sizerMenu",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        (s == "sizerMenu").then_some(Self::SizerMenu)
    }
}

pub fn parse(text: &str) -> Result<ResizeConfig> {
    let ConfigPart { bindings, .. } =
        window_core::config::parse(text, (), ResizeAction::parse, |_, _, _| Ok(false))?;
    Ok(ResizeConfig { bindings })
}
