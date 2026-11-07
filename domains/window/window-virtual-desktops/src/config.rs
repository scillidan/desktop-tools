use anyhow::Result;
use window_core::{Binding, ConfigPart};

pub const DEFAULT_CONFIG: &str = include_str!("../window-virtual-desktops.toml");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VirtualDesktopAction {
    NextDesktop,
    PrevDesktop,
    SwitchToDesktop(u8),
    MoveToDesktop(u8),
}

impl VirtualDesktopAction {
    pub fn as_str(self) -> String {
        match self {
            Self::NextDesktop => "nextDesktop".into(),
            Self::PrevDesktop => "prevDesktop".into(),
            Self::SwitchToDesktop(n) => format!("switchToDesktop{n}"),
            Self::MoveToDesktop(n) => format!("moveToDesktop{n}"),
        }
    }

    fn parse(s: &str) -> Option<Self> {
        let numbered = |prefix: &str| -> Option<u8> {
            let n = s.strip_prefix(prefix)?.parse::<u8>().ok()?;
            (1..=9).contains(&n).then_some(n)
        };
        Some(match s {
            "nextDesktop" => Self::NextDesktop,
            "prevDesktop" => Self::PrevDesktop,
            _ => {
                if let Some(n) = numbered("switchToDesktop") {
                    Self::SwitchToDesktop(n)
                } else {
                    Self::MoveToDesktop(numbered("moveToDesktop")?)
                }
            }
        })
    }
}

#[derive(Default)]
pub struct VirtualDesktopConfig {
    pub bindings: Vec<Binding<VirtualDesktopAction>>,
}

pub fn parse(text: &str) -> Result<VirtualDesktopConfig> {
    let ConfigPart { bindings, .. } =
        window_core::config::parse(text, (), VirtualDesktopAction::parse, |_, _, _| Ok(false))?;
    Ok(VirtualDesktopConfig { bindings })
}
