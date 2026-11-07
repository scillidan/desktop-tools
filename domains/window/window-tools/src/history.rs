use anyhow::Result;
use window_core::{UndoEntry, WindowHistory, WindowOps};

const UNDO_CAP: usize = 32;

#[derive(Default)]
pub struct History {
    entries: Vec<UndoEntry>,
}

impl WindowHistory for History {
    fn record(&mut self, entry: UndoEntry) {
        if self.entries.len() == UNDO_CAP {
            self.entries.remove(0);
        }
        self.entries.push(entry);
    }
}

impl History {
    pub fn undo<P: WindowOps>(&mut self, p: &P) -> Result<()> {
        let Some(e) = self.entries.pop() else {
            anyhow::bail!("nothing to undo");
        };
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
