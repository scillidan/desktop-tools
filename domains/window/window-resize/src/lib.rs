pub mod config;

use anyhow::{Context, Result};
use window_core::{target, Rect, SizeSpec, SizerPick, WinId, WindowHistory, WindowOps};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeAction {
    SizerMenu,
}

#[derive(Default)]
pub struct Resize {
    sizer: Vec<SizeSpec>,
    sizer_idx: usize,
}

impl Resize {
    pub fn set_sizer(&mut self, presets: Vec<SizeSpec>) {
        self.sizer = presets;
        self.sizer_idx = 0;
    }

    pub fn apply<P: WindowOps>(
        &mut self,
        p: &P,
        history: &mut dyn WindowHistory,
        forced: Option<WinId>,
    ) -> Result<()> {
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
        let new_pos = Rect {
            l: rect.l,
            t: rect.t,
            r: rect.l + w + frame.l - rect.l + rect.r - frame.r,
            b: rect.t + h + frame.t - rect.t + rect.b - frame.b,
        };
        if new_pos == rect {
            return Ok(());
        }
        p.move_window(t, new_pos)?;
        history.record(window_core::UndoEntry {
            id: t,
            prev: Some(rect),
            minimized: false,
        });
        Ok(())
    }
}
