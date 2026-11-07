//! Window-domain primitives shared by feature modules, without host/UI state.

use anyhow::{bail, Context, Result};
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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SizerPick {
    Picked(usize),
    Cancelled,
    Unsupported,
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

/// A Sizer preset: exact pixels, or a fraction applied to both dimensions.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SizeSpec {
    Px { w: i32, h: i32 },
    Fraction { num: u32, den: u32 },
}

impl SizeSpec {
    pub fn resolve(self, work: Rect) -> (i32, i32) {
        match self {
            Self::Px { w, h } => (w, h),
            Self::Fraction { num, den } => {
                let w = (work.w() as i64 * num as i64 / den as i64) as i32;
                let h = (work.h() as i64 * num as i64 / den as i64) as i32;
                (w.max(1), h.max(1))
            }
        }
    }
}

impl FromStr for SizeSpec {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        let s = s.trim();
        if let Some((w, h)) = s.split_once(['x', 'X']) {
            let w: i32 = w
                .trim()
                .parse()
                .with_context(|| format!("invalid width in `{s}`"))?;
            let h: i32 = h
                .trim()
                .parse()
                .with_context(|| format!("invalid height in `{s}`"))?;
            anyhow::ensure!((1..=20_000).contains(&w), "width must be 1..=20000");
            anyhow::ensure!((1..=20_000).contains(&h), "height must be 1..=20000");
            return Ok(Self::Px { w, h });
        }
        let (num, den) = s
            .split_once('/')
            .with_context(|| format!("preset `{s}` must be WxH (1600x1200) or N/D (1/2)"))?;
        let num: u32 = num
            .trim()
            .parse()
            .with_context(|| format!("invalid numerator in `{s}`"))?;
        let den: u32 = den
            .trim()
            .parse()
            .with_context(|| format!("invalid denominator in `{s}`"))?;
        anyhow::ensure!(
            num > 0 && num <= den && den <= 1000,
            "fraction must be 1 <= N <= D <= 1000"
        );
        Ok(Self::Fraction { num, den })
    }
}

impl std::fmt::Display for SizeSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Px { w, h } => write!(f, "{w}x{h}"),
            Self::Fraction { num, den } => write!(f, "{num}/{den}"),
        }
    }
}

/// Platform operations shared by the window feature state machines.
pub trait WindowOps: WindowQuery {
    fn monitors(&self) -> Vec<Monitor>;
    fn monitor_index(&self, id: WinId) -> usize;
    fn window_rect(&self, id: WinId) -> Option<Rect>;
    fn visible_frame(&self, id: WinId) -> Option<Rect>;
    fn move_window(&self, id: WinId, rect: Rect) -> Result<()>;
    fn is_maximized(&self, id: WinId) -> bool;
    fn set_maximized(&self, id: WinId, max: bool) -> Result<()>;
    fn is_always_on_top(&self, id: WinId) -> bool;
    fn set_always_on_top(&self, id: WinId, on: bool) -> Result<()>;
    fn is_always_on_bottom(&self, id: WinId) -> bool;
    fn set_always_on_bottom(&self, id: WinId, on: bool) -> Result<()>;
    fn minimize(&self, id: WinId) -> Result<()>;
    fn restore(&self, id: WinId) -> Result<()>;
    fn unpin_all(&self) -> Result<()>;
    fn pick_sizer(&self, presets: &[SizeSpec]) -> SizerPick;
}

/// One reversible window operation, shared by geometry and state features.
#[derive(Clone, Copy)]
pub struct UndoEntry {
    pub id: WinId,
    pub prev: Option<Rect>,
    pub minimized: bool,
}

pub trait WindowHistory {
    fn record(&mut self, entry: UndoEntry);
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
pub mod config;
pub mod keys;
pub mod win;
pub use config::{Binding, ConfigPart};
pub use keys::{Combo, Key};
