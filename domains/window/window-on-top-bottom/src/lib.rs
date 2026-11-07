use anyhow::Result;
use window_core::{target, WinId, WindowOps};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnTopBottomAction {
    ToggleAlwaysOnTop,
    ToggleAlwaysOnBottom,
    ClearAllPins,
}

#[derive(Default)]
pub struct OnTopBottom;

impl OnTopBottom {
    pub fn apply<P: WindowOps>(
        &mut self,
        p: &P,
        action: OnTopBottomAction,
        forced: Option<WinId>,
    ) -> Result<()> {
        if action == OnTopBottomAction::ClearAllPins {
            return p.unpin_all();
        }
        let t = target(p, forced)?;
        match action {
            OnTopBottomAction::ToggleAlwaysOnTop => p.set_always_on_top(t, !p.is_always_on_top(t)),
            OnTopBottomAction::ToggleAlwaysOnBottom => {
                p.set_always_on_bottom(t, !p.is_always_on_bottom(t))
            }
            OnTopBottomAction::ClearAllPins => unreachable!(),
        }
    }
}
