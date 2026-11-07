use crate::MaximizeMinimizeAction;
use anyhow::Result;
use window_core::{Binding, ConfigPart};

pub const DEFAULT_CONFIG: &str = include_str!("../window-maximize-minimize.toml");

#[derive(Default)]
pub struct MaximizeMinimizeConfig {
    pub bindings: Vec<Binding<MaximizeMinimizeAction>>,
}

impl MaximizeMinimizeAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Maximize => "maximize",
            Self::Minimize => "minimize",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "maximize" => Self::Maximize,
            "minimize" => Self::Minimize,
            _ => return None,
        })
    }
}

pub fn parse(text: &str) -> Result<MaximizeMinimizeConfig> {
    let ConfigPart { bindings, .. } =
        window_core::config::parse(text, (), MaximizeMinimizeAction::parse, |_, _, _| Ok(false))?;
    Ok(MaximizeMinimizeConfig { bindings })
}
