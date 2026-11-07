//! Rectangle snapping and monitor movement.

use crate::geometry::{center, inset, merge, scale_to, to_bottom, to_left, to_right, to_top, Rect};
use anyhow::{Context, Result};
use window_core::{target, WinId, WindowHistory, WindowOps};

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
}

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
        }
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
