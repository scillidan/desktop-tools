//! Same-application window cycling with a stable Z-order snapshot.

use anyhow::{Context, Result};
use window_core::{WinId, WindowQuery};

pub trait SwitcherPlatform: WindowQuery {
    /// Visible windows of the same app, top of Z-order first.
    fn same_app_windows(&self, id: WinId) -> Vec<WinId>;

    fn focus(&self, id: WinId) -> Result<()>;
}

#[derive(Default)]
pub struct Switcher {
    cycle_windows: Vec<WinId>,
    cycle_idx: usize,
}

impl Switcher {
    /// Cycle to the next/previous visible window of the same process.
    pub fn switch<P: SwitcherPlatform>(
        &mut self,
        p: &P,
        dir: isize,
        forced: Option<WinId>,
    ) -> Result<()> {
        let fg = match forced {
            Some(w) if p.is_zonable(w) => w,
            _ => p
                .foreground()
                .filter(|&w| p.is_zonable(w))
                .or_else(|| p.last_active())
                .context("no foreground window")?,
        };
        if !self.cycle_windows.contains(&fg) {
            // Focusing raises a window. Re-querying Z-order on every press
            // would ping-pong between only two windows.
            self.cycle_windows = p.same_app_windows(fg);
            self.cycle_idx = self
                .cycle_windows
                .iter()
                .position(|&w| w == fg)
                .unwrap_or(0);
        }
        let current = self.cycle_windows.get(self.cycle_idx).copied();
        self.cycle_windows.retain(|&w| p.is_zonable(w));
        if let Some(w) = current {
            if let Some(i) = self.cycle_windows.iter().position(|&x| x == w) {
                self.cycle_idx = i;
            }
        }
        anyhow::ensure!(
            !self.cycle_windows.is_empty(),
            "no windows to switch between"
        );
        let len = self.cycle_windows.len() as isize;
        self.cycle_idx = (self.cycle_idx as isize + dir).rem_euclid(len) as usize;
        p.focus(self.cycle_windows[self.cycle_idx])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashSet;

    #[derive(Default)]
    struct Mock {
        fg: Option<WinId>,
        zonable: HashSet<WinId>,
        windows: Vec<WinId>,
        focused: RefCell<Vec<WinId>>,
    }

    impl WindowQuery for Mock {
        fn foreground(&self) -> Option<WinId> {
            self.fg
        }

        fn is_zonable(&self, id: WinId) -> bool {
            self.zonable.contains(&id)
        }
    }

    impl SwitcherPlatform for Mock {
        fn same_app_windows(&self, _id: WinId) -> Vec<WinId> {
            self.windows.clone()
        }

        fn focus(&self, id: WinId) -> Result<()> {
            // The production backend is &self too; interior mutability keeps
            // this test focused on the cycling order rather than mock plumbing.
            self.focused.borrow_mut().push(id);
            Ok(())
        }
    }

    #[test]
    fn next_window_cycles_the_foreground_app() {
        let zonable = HashSet::from([1, 2, 3]);
        let mock = Mock {
            fg: Some(1),
            zonable,
            windows: vec![1, 2, 3],
            ..Default::default()
        };
        let mut switcher = Switcher::default();
        switcher.switch(&mock, 1, None).unwrap();
        switcher.switch(&mock, 1, None).unwrap();
        switcher.switch(&mock, 1, None).unwrap();
        switcher.switch(&mock, -1, None).unwrap();
        assert_eq!(mock.focused.into_inner(), vec![2, 3, 1, 3]);
    }
}
