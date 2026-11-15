use anyhow::Result;
use window_core::{UndoEntry, WindowHistory, WindowOps};

const UNDO_CAP: usize = 32;

enum Group {
    One(UndoEntry),
    Pair(UndoEntry, UndoEntry),
}

#[derive(Default)]
pub struct History {
    entries: Vec<Group>,
}

impl History {
    fn push(&mut self, group: Group) {
        if self.entries.len() == UNDO_CAP {
            self.entries.remove(0);
        }
        self.entries.push(group);
    }

    fn restore<P: WindowOps>(p: &P, e: UndoEntry) -> Result<()> {
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

    pub fn undo<P: WindowOps>(&mut self, p: &P) -> Result<()> {
        let Some(group) = self.entries.pop() else {
            anyhow::bail!("nothing to undo");
        };
        match group {
            Group::One(e) => Self::restore(p, e),
            Group::Pair(a, b) => {
                let a_result = Self::restore(p, a);
                let b_result = Self::restore(p, b);
                a_result.and(b_result)
            }
        }
    }
}

impl WindowHistory for History {
    fn record(&mut self, entry: UndoEntry) {
        self.push(Group::One(entry));
    }

    fn record_pair(&mut self, a: UndoEntry, b: UndoEntry) {
        self.push(Group::Pair(a, b));
    }
}
