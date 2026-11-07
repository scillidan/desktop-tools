pub mod config;

use anyhow::Result;
use window_core::{target, UndoEntry, WinId, WindowHistory, WindowOps};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaximizeMinimizeAction {
    Maximize,
    Minimize,
}

#[derive(Default)]
pub struct MaximizeMinimize;

impl MaximizeMinimize {
    pub fn apply<P: WindowOps>(
        &mut self,
        p: &P,
        history: &mut dyn WindowHistory,
        action: MaximizeMinimizeAction,
        forced: Option<WinId>,
    ) -> Result<()> {
        let t = target(p, forced)?;
        match action {
            MaximizeMinimizeAction::Maximize => {
                if p.is_maximized(t) {
                    return p.set_maximized(t, false);
                }
                let before = p.window_rect(t);
                p.set_maximized(t, true)?;
                if let Some(before) = before {
                    history.record(UndoEntry {
                        id: t,
                        prev: Some(before),
                        minimized: false,
                    });
                }
                Ok(())
            }
            MaximizeMinimizeAction::Minimize => {
                history.record(UndoEntry {
                    id: t,
                    prev: None,
                    minimized: true,
                });
                p.minimize(t)
            }
        }
    }
}
