//! Window-domain primitives shared by feature modules, without host/UI state.

use anyhow::{bail, Result};
use std::str::FromStr;

/// Opaque window handle: HWND value on Windows, X window id on X11.
pub type WinId = u64;

/// Inclusive-exclusive rect, matching Win32 RECT semantics (right/bottom).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Rect {
    pub l: i32,
    pub t: i32,
    pub r: i32,
    pub b: i32,
}

impl Rect {
    pub fn w(&self) -> i32 {
        self.r - self.l
    }

    pub fn h(&self) -> i32 {
        self.b - self.t
    }

    pub fn has_area(&self) -> bool {
        self.w() > 0 && self.h() > 0
    }
}

#[derive(Clone, Copy)]
pub struct Monitor {
    pub work: Rect,
}

/// Intersection; empty (zero-area) results are fine, callers check `has_area`.
#[cfg_attr(windows, allow(dead_code))]
pub fn intersect(a: Rect, b: Rect) -> Rect {
    Rect {
        l: a.l.max(b.l),
        t: a.t.max(b.t),
        r: a.r.min(b.r),
        b: a.b.min(b.b),
    }
}

/// A modifier-only chord (`win`, `win+alt`) for mouse gestures.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Mods {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub win: bool,
}

impl Mods {
    pub fn is_empty(&self) -> bool {
        !(self.ctrl || self.shift || self.alt || self.win)
    }
}

impl FromStr for Mods {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Mods> {
        let mut m = Mods::default();
        for part in s.split('+') {
            match part.trim().to_ascii_lowercase().as_str() {
                "" => continue,
                "ctrl" | "control" | "^" => m.ctrl = true,
                "shift" => m.shift = true,
                "alt" | "!" => m.alt = true,
                "win" | "meta" | "super" | "#" => m.win = true,
                other => bail!("`{other}` is not a modifier in `{s}`"),
            }
        }
        anyhow::ensure!(
            !m.is_empty(),
            "`{s}` needs at least one of ctrl/shift/alt/win"
        );
        Ok(m)
    }
}

/// Narrow foreground/target queries shared by window feature state machines.
pub trait WindowQuery {
    fn foreground(&self) -> Option<WinId>;

    fn last_active(&self) -> Option<WinId> {
        None
    }

    fn is_zonable(&self, id: WinId) -> bool;
}

/// Rectangle/vdesk target selection; switcher intentionally has its own fallback.
pub fn target<Q: WindowQuery + ?Sized>(q: &Q, forced: Option<WinId>) -> Result<WinId> {
    if let Some(w) = forced.filter(|&w| q.is_zonable(w)) {
        return Ok(w);
    }
    if let Some(w) = q.foreground().filter(|&w| q.is_zonable(w)) {
        return Ok(w);
    }
    if let Some(w) = q.last_active().filter(|&w| q.is_zonable(w)) {
        return Ok(w);
    }
    bail!("no zonable target window in foreground")
}

#[cfg(windows)]
pub mod win;
