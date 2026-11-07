//! Rectangle snapping, pin/minimize state, Sizer presets and shared undo.

use crate::geometry::{
    center, inset, merge, scale_to, to_bottom, to_left, to_right, to_top, Rect, SizeSpec,
};
use anyhow::{Context, Result};
use window_core::{target, Monitor, WinId, WindowQuery};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SizerPick {
    Picked(usize),
    Cancelled,
    Unsupported,
}

pub trait RectanglePlatform: WindowQuery {
    fn monitors(&self) -> Vec<Monitor>;
    fn monitor_index(&self, id: WinId) -> usize;
    fn window_rect(&self, id: WinId) -> Option<Rect>;
    fn visible_frame(&self, id: WinId) -> Option<Rect>;
    fn move_window(&self, id: WinId, r: Rect) -> Result<()>;
    fn is_maximized(&self, id: WinId) -> bool;
    fn set_maximized(&self, id: WinId, on: bool) -> Result<()>;
    fn is_always_on_top(&self, id: WinId) -> bool;
    fn set_always_on_top(&self, id: WinId, on: bool) -> Result<()>;
    fn is_always_on_bottom(&self, id: WinId) -> bool;
    fn set_always_on_bottom(&self, id: WinId, on: bool) -> Result<()>;
    fn minimize(&self, id: WinId) -> Result<()>;
    fn restore(&self, id: WinId) -> Result<()>;
    fn unpin_all(&self) -> Result<()>;
    fn pick_sizer(&self, presets: &[SizeSpec]) -> SizerPick;
}

#[derive(Clone, Copy)]
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
    Maximize,
    NextDisplay,
    PrevDisplay,
    ToggleAlwaysOnTop,
    ToggleAlwaysOnBottom,
    Minimize,
    UnpinAll,
    SizerMenu,
    Undo,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Group {
    Edge,
    Corner,
}

#[derive(Clone, Copy)]
struct UndoEntry {
    id: WinId,
    prev: Option<Rect>,
    minimized: bool,
}

#[derive(Default)]
pub struct Rectangle {
    last_cycle: Option<(WinId, Group, usize)>,
    edge_turns: [u32; 4],
    corner_turns: [u32; 4],
    undo: Vec<UndoEntry>,
    sizer: Vec<SizeSpec>,
    sizer_idx: usize,
    gap: i32,
}

const UNDO_CAP: usize = 32;

impl Rectangle {
    pub fn set_gap(&mut self, gap: i32) {
        self.gap = gap.max(0);
    }

    pub fn set_sizer(&mut self, presets: Vec<SizeSpec>) {
        self.sizer = presets;
        self.sizer_idx = 0;
    }

    pub fn apply<P: RectanglePlatform>(
        &mut self,
        p: &P,
        action: RectangleAction,
        forced: Option<WinId>,
    ) -> Result<()> {
        match action {
            RectangleAction::MoveToLeft => self.cycle(p, Group::Edge, 0, forced),
            RectangleAction::MoveToRight => self.cycle(p, Group::Edge, 1, forced),
            RectangleAction::MoveToTop => self.cycle(p, Group::Edge, 2, forced),
            RectangleAction::MoveToBottom => self.cycle(p, Group::Edge, 3, forced),
            RectangleAction::MoveToTopLeft => self.cycle(p, Group::Corner, 0, forced),
            RectangleAction::MoveToTopRight => self.cycle(p, Group::Corner, 1, forced),
            RectangleAction::MoveToBottomLeft => self.cycle(p, Group::Corner, 2, forced),
            RectangleAction::MoveToBottomRight => self.cycle(p, Group::Corner, 3, forced),
            RectangleAction::MoveToCenter => {
                let t = target(p, forced)?;
                self.resize(p, t, center, 0)?;
                Ok(())
            }
            RectangleAction::Maximize => {
                self.last_cycle = None;
                let t = target(p, forced)?;
                if p.is_maximized(t) {
                    return p.set_maximized(t, false);
                }
                let before = p.window_rect(t);
                p.set_maximized(t, true)?;
                if let Some(before) = before {
                    self.record_undo(t, Some(before), false);
                }
                Ok(())
            }
            RectangleAction::NextDisplay => {
                let t = target(p, forced)?;
                self.resize(p, t, center, 1)?;
                Ok(())
            }
            RectangleAction::PrevDisplay => {
                let t = target(p, forced)?;
                self.resize(p, t, center, -1)?;
                Ok(())
            }
            RectangleAction::ToggleAlwaysOnTop => {
                let t = target(p, forced)?;
                let on = !p.is_always_on_top(t);
                p.set_always_on_top(t, on)
            }
            RectangleAction::ToggleAlwaysOnBottom => {
                let t = target(p, forced)?;
                let on = !p.is_always_on_bottom(t);
                p.set_always_on_bottom(t, on)
            }
            RectangleAction::Minimize => {
                self.last_cycle = None;
                let t = target(p, forced)?;
                self.record_undo(t, None, true);
                p.minimize(t)
            }
            RectangleAction::UnpinAll => p.unpin_all(),
            RectangleAction::SizerMenu => self.apply_sizer(p, forced),
            RectangleAction::Undo => {
                let Some(e) = self.undo.pop() else {
                    anyhow::bail!("nothing to undo");
                };
                self.last_cycle = None;
                if e.minimized {
                    if p.window_rect(e.id).is_none() {
                        anyhow::bail!("that window is gone");
                    }
                    return p.restore(e.id);
                }
                if !p.is_zonable(e.id) {
                    anyhow::bail!("that window is gone");
                }
                let Some(prev) = e.prev else {
                    return Ok(());
                };
                p.move_window(e.id, prev)
            }
        }
    }

    fn record_undo(&mut self, id: WinId, prev: Option<Rect>, minimized: bool) {
        if self.undo.len() == UNDO_CAP {
            self.undo.remove(0);
        }
        self.undo.push(UndoEntry {
            id,
            prev,
            minimized,
        });
    }

    fn apply_sizer<P: RectanglePlatform>(&mut self, p: &P, forced: Option<WinId>) -> Result<()> {
        anyhow::ensure!(
            !self.sizer.is_empty(),
            "no sizer presets; add entries to resize.toml next to window-tools.toml"
        );
        let idx = match p.pick_sizer(&self.sizer) {
            SizerPick::Picked(i) => i,
            SizerPick::Cancelled => return Ok(()),
            SizerPick::Unsupported => {
                let i = self.sizer_idx % self.sizer.len();
                self.sizer_idx = (i + 1) % self.sizer.len();
                i
            }
        };
        let spec = *self
            .sizer
            .get(idx)
            .context("sizer picker returned an invalid preset")?;
        let t = target(p, forced)?;
        let mons = p.monitors();
        anyhow::ensure!(!mons.is_empty(), "no monitors");
        let work = mons
            .get(p.monitor_index(t))
            .context("monitor index out of range")?
            .work;
        let (w, h) = spec.resolve(work);
        let rect = p.window_rect(t).context("cannot query window rect")?;
        let frame = p.visible_frame(t).unwrap_or(rect);
        let l_extra = frame.l - rect.l;
        let t_extra = frame.t - rect.t;
        let r_extra = rect.r - frame.r;
        let b_extra = rect.b - frame.b;
        let new_pos = Rect {
            l: rect.l,
            t: rect.t,
            r: rect.l + w + l_extra + r_extra,
            b: rect.t + h + t_extra + b_extra,
        };
        if new_pos == rect {
            return Ok(());
        }
        self.last_cycle = None;
        p.move_window(t, new_pos)?;
        self.record_undo(t, Some(rect), false);
        Ok(())
    }

    /// Half -> two thirds -> one third cycle, RectangleWin semantics.
    fn cycle<P: RectanglePlatform>(
        &mut self,
        p: &P,
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
        // Corners cycle width only; height stays half (RectangleWin merge).
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
        self.resize(p, hwnd, |work, _| f(work), 0)?;
        self.last_cycle = Some((hwnd, group, i));
        Ok(())
    }

    /// Work-area snap with invisible-border compensation and monitor scaling.
    fn resize<P: RectanglePlatform>(
        &mut self,
        p: &P,
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
        self.record_undo(hwnd, Some(rect), false);
        Ok(new_pos)
    }
}
