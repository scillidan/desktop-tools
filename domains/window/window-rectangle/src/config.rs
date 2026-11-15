use crate::RectangleAction;
use anyhow::{Context, Result};
use window_core::{Binding, ConfigPart};

pub const DEFAULT_CONFIG: &str = include_str!("../window-rectangle.toml");

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RectangleSettings {
    pub gap: i32,
}

pub struct RectangleConfig {
    pub bindings: Vec<Binding<RectangleAction>>,
    pub settings: RectangleSettings,
}

impl RectangleAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MoveToLeft => "moveToLeft",
            Self::MoveToRight => "moveToRight",
            Self::MoveToTop => "moveToTop",
            Self::MoveToBottom => "moveToBottom",
            Self::MoveToTopLeft => "moveToTopLeft",
            Self::MoveToTopRight => "moveToTopRight",
            Self::MoveToBottomLeft => "moveToBottomLeft",
            Self::MoveToBottomRight => "moveToBottomRight",
            Self::MoveToCenter => "moveToCenter",
            Self::NextDisplay => "nextDisplay",
            Self::PrevDisplay => "prevDisplay",
            Self::SwapWindows => "swapWindows",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "moveToLeft" => Self::MoveToLeft,
            "moveToRight" => Self::MoveToRight,
            "moveToTop" => Self::MoveToTop,
            "moveToBottom" => Self::MoveToBottom,
            "moveToTopLeft" => Self::MoveToTopLeft,
            "moveToTopRight" => Self::MoveToTopRight,
            "moveToBottomLeft" => Self::MoveToBottomLeft,
            "moveToBottomRight" => Self::MoveToBottomRight,
            "moveToCenter" => Self::MoveToCenter,
            "nextDisplay" => Self::NextDisplay,
            "prevDisplay" => Self::PrevDisplay,
            "swapWindows" => Self::SwapWindows,
            _ => return None,
        })
    }
}

pub fn parse(text: &str) -> Result<RectangleConfig> {
    let ConfigPart { bindings, settings } = window_core::config::parse(
        text,
        RectangleSettings::default(),
        RectangleAction::parse,
        |settings, key, value| {
            if key != "gap" {
                return Ok(false);
            }
            settings.gap = value
                .as_integer()
                .context("gap must be an integer")?
                .clamp(0, 4096) as i32;
            Ok(true)
        },
    )?;
    Ok(RectangleConfig { bindings, settings })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swap_is_opt_in_and_parses_its_isolated_binding() {
        let defaults = parse(DEFAULT_CONFIG).unwrap();
        assert!(!defaults
            .bindings
            .iter()
            .any(|b| b.action == RectangleAction::SwapWindows));
        let custom = parse("\"ctrl+win+alt+x\" = \"swapWindows\"").unwrap();
        assert_eq!(custom.bindings.len(), 1);
        assert_eq!(custom.bindings[0].action, RectangleAction::SwapWindows);
    }
}
