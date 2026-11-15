//! Rectangle snapping and monitor movement.

use crate::geometry::{
    center, inset, merge, outer_like, scale_to, to_bottom, to_left, to_right, to_top, Rect,
};
use anyhow::{Context, Result};
use std::time::{Duration, Instant};
use window_core::{target, UndoEntry, WinId, WindowHistory, WindowOps};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RectangleAction {
    MoveToLeft,
    MoveToRight,
    MoveToTop,
    MoveToBottom,
    MoveToTopLeft,
    MoveToTopRight,
    MoveToBottomLeft,
    MoveToBottomRight,
    MoveToCenter,
    NextDisplay,
    PrevDisplay,
    SwapWindows,
}

#[derive(Clone, Copy)]
struct Armed {
    id: WinId,
    deadline: Instant,
}

const SWAP_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Group {
    Edge,
    Corner,
}

#[derive(Default)]
pub struct Rectangle {
    last_cycle: Option<(WinId, Group, usize)>,
    edge_turns: [u32; 4],
    corner_turns: [u32; 4],
    gap: i32,
    swap: Option<Armed>,
}

impl Rectangle {
    pub fn set_gap(&mut self, gap: i32) {
        self.gap = gap.max(0);
    }

    pub fn reset_cycle(&mut self) {
        self.last_cycle = None;
    }

    pub fn apply<P: WindowOps>(
        &mut self,
        p: &P,
        history: &mut dyn WindowHistory,
        action: RectangleAction,
        forced: Option<WinId>,
    ) -> Result<()> {
        self.apply_at(p, history, action, forced, Instant::now())
    }

    fn apply_at<P: WindowOps>(
        &mut self,
        p: &P,
        history: &mut dyn WindowHistory,
        action: RectangleAction,
        forced: Option<WinId>,
        now: Instant,
    ) -> Result<()> {
        if action != RectangleAction::SwapWindows {
            self.cancel_swap();
        } else {
            self.tick_at(now);
        }
        match action {
            RectangleAction::MoveToLeft => self.cycle(p, history, Group::Edge, 0, forced),
            RectangleAction::MoveToRight => self.cycle(p, history, Group::Edge, 1, forced),
            RectangleAction::MoveToTop => self.cycle(p, history, Group::Edge, 2, forced),
            RectangleAction::MoveToBottom => self.cycle(p, history, Group::Edge, 3, forced),
            RectangleAction::MoveToTopLeft => self.cycle(p, history, Group::Corner, 0, forced),
            RectangleAction::MoveToTopRight => self.cycle(p, history, Group::Corner, 1, forced),
            RectangleAction::MoveToBottomLeft => self.cycle(p, history, Group::Corner, 2, forced),
            RectangleAction::MoveToBottomRight => self.cycle(p, history, Group::Corner, 3, forced),
            RectangleAction::MoveToCenter => {
                let t = target(p, forced)?;
                self.resize(p, history, t, center, 0)?;
                Ok(())
            }
            RectangleAction::NextDisplay => {
                let t = target(p, forced)?;
                self.resize(p, history, t, center, 1)?;
                Ok(())
            }
            RectangleAction::PrevDisplay => {
                let t = target(p, forced)?;
                self.resize(p, history, t, center, -1)?;
                Ok(())
            }
            RectangleAction::SwapWindows => self.swap(p, history, forced, now),
        }
    }

    pub fn swap_armed(&self) -> bool {
        self.swap.is_some()
    }

    pub fn cancel_swap(&mut self) {
        self.swap = None;
    }

    pub fn tick_swap(&mut self) {
        self.tick_at(Instant::now());
    }

    fn tick_at(&mut self, now: Instant) {
        if self.swap.is_some_and(|armed| armed.deadline <= now) {
            self.cancel_swap();
        }
    }

    fn eligible<P: WindowOps>(p: &P, id: WinId) -> bool {
        p.is_zonable(id) && !p.is_maximized(id)
    }

    fn swap<P: WindowOps>(
        &mut self,
        p: &P,
        history: &mut dyn WindowHistory,
        forced: Option<WinId>,
        now: Instant,
    ) -> Result<()> {
        let Some(armed) = self.swap else {
            let id = target(p, forced)?;
            anyhow::ensure!(
                Self::eligible(p, id),
                "swap requires a normal visible window"
            );
            self.last_cycle = None;
            self.swap = Some(Armed {
                id,
                deadline: now + SWAP_TIMEOUT,
            });
            return Ok(());
        };

        let result = (|| {
            let b = p.foreground().context("no foreground window for swap")?;
            if b == armed.id {
                return Ok(());
            }
            self.commit_swap(p, history, armed.id, b)
        })();
        self.cancel_swap();
        self.last_cycle = None;
        result
    }

    fn commit_swap<P: WindowOps>(
        &self,
        p: &P,
        history: &mut dyn WindowHistory,
        a: WinId,
        b: WinId,
    ) -> Result<()> {
        anyhow::ensure!(
            Self::eligible(p, a) && Self::eligible(p, b),
            "swap cancelled: both windows must be visible, normal and zonable"
        );
        let a_rect = p.window_rect(a).context("cannot query first window rect")?;
        let b_rect = p
            .window_rect(b)
            .context("cannot query second window rect")?;
        let a_frame = p.visible_frame(a).unwrap_or(a_rect);
        let b_frame = p.visible_frame(b).unwrap_or(b_rect);
        let a_new = outer_like(b_frame, a_rect, a_frame);
        let b_new = outer_like(a_frame, b_rect, b_frame);

        let mut moved = false;
        if a_new != a_rect {
            p.move_window(a, a_new)?;
            moved = true;
        }
        let b_result = if b_new != b_rect {
            p.move_window(b, b_new)
        } else {
            Ok(())
        };
        moved |= b_result.is_ok() && b_new != b_rect;
        if moved {
            history.record_pair(
                UndoEntry {
                    id: a,
                    prev: Some(a_rect),
                    minimized: false,
                },
                UndoEntry {
                    id: b,
                    prev: Some(b_rect),
                    minimized: false,
                },
            );
        }
        b_result
    }

    /// Half -> two thirds -> one third cycle, RectangleWin semantics.
    fn cycle<P: WindowOps>(
        &mut self,
        p: &P,
        history: &mut dyn WindowHistory,
        group: Group,
        i: usize,
        forced: Option<WinId>,
    ) -> Result<()> {
        let hwnd = target(p, forced)?;
        if self.last_cycle != Some((hwnd, group, i)) {
            self.edge_turns = [0; 4];
            self.corner_turns = [0; 4];
        }
        let turns = match group {
            Group::Edge => &mut self.edge_turns,
            Group::Corner => &mut self.corner_turns,
        };
        let (mul, div) = [(1, 2), (2, 3), (1, 3)][(turns[i] % 3) as usize];
        turns[i] += 1;
        for (j, t) in turns.iter_mut().enumerate() {
            if j != i {
                *t = 0;
            }
        }
        let f: Box<dyn Fn(Rect) -> Rect> = match (group, i) {
            (Group::Edge, 0) => Box::new(move |w| to_left(w, mul, div)),
            (Group::Edge, 1) => Box::new(move |w| to_right(w, mul, div)),
            (Group::Edge, 2) => Box::new(move |w| to_top(w, mul, div)),
            (Group::Edge, 3) => Box::new(move |w| to_bottom(w, mul, div)),
            (Group::Corner, 0) => Box::new(move |w| merge(to_left(w, mul, div), to_top(w, 1, 2))),
            (Group::Corner, 1) => Box::new(move |w| merge(to_right(w, mul, div), to_top(w, 1, 2))),
            (Group::Corner, 2) => {
                Box::new(move |w| merge(to_left(w, mul, div), to_bottom(w, 1, 2)))
            }
            (Group::Corner, 3) => {
                Box::new(move |w| merge(to_right(w, mul, div), to_bottom(w, 1, 2)))
            }
            _ => unreachable!(),
        };
        self.resize(p, history, hwnd, |work, _| f(work), 0)?;
        self.last_cycle = Some((hwnd, group, i));
        Ok(())
    }

    /// Work-area snap with invisible-border compensation and monitor scaling.
    fn resize<P: WindowOps>(
        &mut self,
        p: &P,
        history: &mut dyn WindowHistory,
        hwnd: WinId,
        f: impl Fn(Rect, Rect) -> Rect,
        monitor_diff: i32,
    ) -> Result<Rect> {
        let rect = p.window_rect(hwnd).context("cannot query window rect")?;
        let mons = p.monitors();
        anyhow::ensure!(!mons.is_empty(), "no monitors");
        let cur = p.monitor_index(hwnd);
        let idx = if monitor_diff != 0 {
            (cur as i32 + monitor_diff).rem_euclid(mons.len() as i32) as usize
        } else {
            cur
        };
        let work = inset(mons[idx].work, self.gap);
        let frame = p.visible_frame(hwnd).unwrap_or(rect);
        let l_extra = frame.l - rect.l;
        let r_extra = rect.r - frame.r;
        let t_extra = frame.t - rect.t;
        let b_extra = rect.b - frame.b;
        let frame = if monitor_diff != 0 {
            scale_to(frame, inset(mons[cur].work, self.gap), work)
        } else {
            frame
        };
        let mut new_pos = f(work, frame);
        new_pos.l -= l_extra;
        new_pos.t -= t_extra;
        new_pos.r += r_extra;
        new_pos.b += b_extra;
        self.last_cycle = None;
        if new_pos == rect {
            return Ok(new_pos);
        }
        p.move_window(hwnd, new_pos)?;
        history.record(window_core::UndoEntry {
            id: hwnd,
            prev: Some(rect),
            minimized: false,
        });
        Ok(new_pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::collections::BTreeMap;
    use window_core::{Monitor, WindowQuery};

    #[derive(Clone, Copy)]
    struct TestWindow {
        outer: Rect,
        visible: Rect,
        zonable: bool,
        maximized: bool,
    }

    struct Mock {
        foreground: Cell<WinId>,
        windows: RefCell<BTreeMap<WinId, TestWindow>>,
    }

    impl Mock {
        fn new(windows: [(WinId, TestWindow); 2]) -> Self {
            Self {
                foreground: Cell::new(windows[0].0),
                windows: RefCell::new(windows.into_iter().collect()),
            }
        }

        fn outer(&self, id: WinId) -> Rect {
            self.windows.borrow()[&id].outer
        }
    }

    impl WindowQuery for Mock {
        fn foreground(&self) -> Option<WinId> {
            Some(self.foreground.get())
        }

        fn is_zonable(&self, id: WinId) -> bool {
            self.windows.borrow().get(&id).is_some_and(|w| w.zonable)
        }
    }

    impl WindowOps for Mock {
        fn monitors(&self) -> Vec<Monitor> {
            vec![Monitor {
                work: Rect {
                    l: -2000,
                    t: -1000,
                    r: 4000,
                    b: 3000,
                },
            }]
        }
        fn monitor_index(&self, _id: WinId) -> usize {
            0
        }
        fn window_rect(&self, id: WinId) -> Option<Rect> {
            self.windows.borrow().get(&id).map(|w| w.outer)
        }
        fn visible_frame(&self, id: WinId) -> Option<Rect> {
            self.windows.borrow().get(&id).map(|w| w.visible)
        }
        fn move_window(&self, id: WinId, r: Rect) -> Result<()> {
            let mut windows = self.windows.borrow_mut();
            let w = windows.get_mut(&id).context("window is gone")?;
            let insets = (
                w.visible.l - w.outer.l,
                w.visible.t - w.outer.t,
                w.visible.r - w.outer.r,
                w.visible.b - w.outer.b,
            );
            w.visible = Rect {
                l: r.l + insets.0,
                t: r.t + insets.1,
                r: r.r + insets.2,
                b: r.b + insets.3,
            };
            w.outer = r;
            Ok(())
        }
        fn is_maximized(&self, id: WinId) -> bool {
            self.windows.borrow().get(&id).is_some_and(|w| w.maximized)
        }
        fn set_maximized(&self, _id: WinId, _on: bool) -> Result<()> {
            Ok(())
        }
        fn is_always_on_top(&self, _id: WinId) -> bool {
            false
        }
        fn set_always_on_top(&self, _id: WinId, _on: bool) -> Result<()> {
            Ok(())
        }
        fn is_always_on_bottom(&self, _id: WinId) -> bool {
            false
        }
        fn set_always_on_bottom(&self, _id: WinId, _on: bool) -> Result<()> {
            Ok(())
        }
        fn minimize(&self, _id: WinId) -> Result<()> {
            Ok(())
        }
        fn restore(&self, _id: WinId) -> Result<()> {
            Ok(())
        }
        fn unpin_all(&self) -> Result<()> {
            Ok(())
        }
        fn pick_sizer(&self, _presets: &[window_core::SizeSpec]) -> window_core::SizerPick {
            window_core::SizerPick::Unsupported
        }
    }

    #[derive(Default)]
    struct TestHistory(Vec<Vec<UndoEntry>>);

    impl WindowHistory for TestHistory {
        fn record(&mut self, entry: UndoEntry) {
            self.0.push(vec![entry]);
        }
        fn record_pair(&mut self, a: UndoEntry, b: UndoEntry) {
            self.0.push(vec![a, b]);
        }
    }

    fn window(outer: Rect, visible: Rect) -> TestWindow {
        TestWindow {
            outer,
            visible,
            zonable: true,
            maximized: false,
        }
    }

    #[test]
    fn swap_uses_visible_slots_and_records_one_pair() {
        let a_outer = Rect {
            l: 0,
            t: 0,
            r: 410,
            b: 310,
        };
        let a_visible = Rect {
            l: 10,
            t: 10,
            r: 400,
            b: 300,
        };
        let b_outer = Rect {
            l: 1000,
            t: 500,
            r: 1200,
            b: 650,
        };
        let b_visible = Rect {
            l: 1005,
            t: 505,
            r: 1195,
            b: 645,
        };
        let p = Mock::new([
            (1, window(a_outer, a_visible)),
            (2, window(b_outer, b_visible)),
        ]);
        let mut rectangle = Rectangle::default();
        let mut history = TestHistory::default();

        rectangle
            .apply(&p, &mut history, RectangleAction::SwapWindows, None)
            .unwrap();
        assert!(rectangle.swap_armed());
        assert!(history.0.is_empty());
        p.foreground.set(2);
        rectangle
            .apply(&p, &mut history, RectangleAction::SwapWindows, None)
            .unwrap();

        assert!(!rectangle.swap_armed());
        assert_eq!(
            p.outer(1),
            Rect {
                l: 995,
                t: 495,
                r: 1205,
                b: 655
            }
        );
        assert_eq!(p.windows.borrow()[&1].visible, b_visible);
        assert_eq!(
            p.outer(2),
            Rect {
                l: 5,
                t: 5,
                r: 405,
                b: 305
            }
        );
        assert_eq!(p.windows.borrow()[&2].visible, a_visible);
        assert_eq!(history.0.len(), 1);
        assert_eq!(history.0[0].len(), 2);
        assert_eq!(history.0[0][0].prev, Some(a_outer));
        assert_eq!(history.0[0][1].prev, Some(b_outer));
    }

    #[test]
    fn same_window_cancels_without_geometry_or_history() {
        let r = Rect {
            l: 0,
            t: 0,
            r: 400,
            b: 300,
        };
        let other = Rect {
            l: 500,
            t: 100,
            r: 900,
            b: 400,
        };
        let p = Mock::new([(1, window(r, r)), (2, window(other, other))]);
        let mut rectangle = Rectangle::default();
        let mut history = TestHistory::default();
        rectangle
            .apply(&p, &mut history, RectangleAction::SwapWindows, None)
            .unwrap();
        rectangle
            .apply(&p, &mut history, RectangleAction::SwapWindows, None)
            .unwrap();
        assert!(!rectangle.swap_armed());
        assert_eq!(p.outer(1), r);
        assert!(history.0.is_empty());
    }

    #[test]
    fn timeout_and_other_rectangle_actions_cancel_armed_state() {
        let r = Rect {
            l: 0,
            t: 0,
            r: 400,
            b: 300,
        };
        let other = Rect {
            l: 500,
            t: 100,
            r: 900,
            b: 400,
        };
        let p = Mock::new([(1, window(r, r)), (2, window(other, other))]);
        let mut rectangle = Rectangle::default();
        let mut history = TestHistory::default();
        let now = Instant::now();
        rectangle
            .apply_at(&p, &mut history, RectangleAction::SwapWindows, None, now)
            .unwrap();
        rectangle.tick_at(now + SWAP_TIMEOUT);
        assert!(!rectangle.swap_armed());

        rectangle
            .apply_at(&p, &mut history, RectangleAction::SwapWindows, None, now)
            .unwrap();
        rectangle
            .apply_at(&p, &mut history, RectangleAction::MoveToCenter, None, now)
            .unwrap();
        assert!(!rectangle.swap_armed());
    }

    #[test]
    fn invalid_second_window_cancels_before_any_move() {
        let r = Rect {
            l: 0,
            t: 0,
            r: 400,
            b: 300,
        };
        let other = Rect {
            l: 500,
            t: 100,
            r: 900,
            b: 400,
        };
        let mut maximized = window(other, other);
        maximized.maximized = true;
        let p = Mock::new([(1, window(r, r)), (2, maximized)]);
        let mut rectangle = Rectangle::default();
        let mut history = TestHistory::default();
        rectangle
            .apply(&p, &mut history, RectangleAction::SwapWindows, None)
            .unwrap();
        p.foreground.set(2);
        assert!(rectangle
            .apply(&p, &mut history, RectangleAction::SwapWindows, None)
            .is_err());
        assert!(!rectangle.swap_armed());
        assert_eq!(p.outer(1), r);
        assert_eq!(p.outer(2), other);
        assert!(history.0.is_empty());
    }
}
