//! Multi-step hotkey sequences; platform-independent matching.

use crate::keys::Combo;
use anyhow::{bail, Result};
use std::fmt;
use std::str::FromStr;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Sequence {
    steps: Vec<Combo>,
}

impl FromStr for Sequence {
    type Err = anyhow::Error;

    fn from_str(text: &str) -> Result<Self> {
        let steps = text
            .split('>')
            .map(|step| step.trim().parse())
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { steps })
    }
}

impl fmt::Display for Sequence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = self
            .steps
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" > ");
        f.write_str(&text)
    }
}

struct Pending {
    candidates: Vec<usize>,
    next: usize,
    deadline: Instant,
}

pub struct SequenceSet<A> {
    bindings: Vec<(A, Sequence)>,
    timeout: Duration,
    pending: Option<Pending>,
}

impl<A> SequenceSet<A> {
    pub fn new(bindings: Vec<(A, Sequence)>, timeout: Duration) -> Result<Self> {
        for (_, sequence) in &bindings {
            for step in &sequence.steps {
                if !(step.ctrl || step.alt || step.win) {
                    bail!(
                        "hotkey sequence `{sequence}` has a step without Ctrl, Alt or Win; \
                         plain keys and Shift-only combos cannot stay globally registered"
                    );
                }
            }
        }
        for left in 0..bindings.len() {
            for right in left + 1..bindings.len() {
                let a = &bindings[left].1.steps;
                let b = &bindings[right].1.steps;
                let prefix = (a.len() <= b.len() && a.as_slice() == &b[..a.len()])
                    || (b.len() <= a.len() && b.as_slice() == &a[..b.len()]);
                if prefix {
                    bail!(
                        "hotkey sequences `{}` and `{}` are duplicate or prefix-ambiguous",
                        bindings[left].1,
                        bindings[right].1
                    );
                }
            }
        }
        for (_, single) in bindings
            .iter()
            .filter(|(_, sequence)| sequence.steps.len() == 1)
        {
            if bindings.iter().any(|(_, sequence)| {
                sequence.steps.len() > 1 && sequence.steps.contains(&single.steps[0])
            }) {
                bail!(
                    "single hotkey `{}` also occurs in a multi-step sequence",
                    single
                );
            }
        }
        Ok(Self {
            bindings,
            timeout,
            pending: None,
        })
    }

    /// Unique combos for the platform hotkey adapter to register, in first-use order.
    pub fn combos(&self) -> Vec<Combo> {
        let mut combos = Vec::new();
        for combo in self
            .bindings
            .iter()
            .flat_map(|(_, sequence)| sequence.steps.iter())
        {
            if !combos.contains(combo) {
                combos.push(*combo);
            }
        }
        combos
    }

    pub fn reset(&mut self) {
        self.pending = None;
    }
}

impl<A: Clone> SequenceSet<A> {
    pub fn press(&mut self, combo: Combo, now: Instant) -> Option<A> {
        if self.pending.as_ref().is_some_and(|p| now >= p.deadline) {
            self.pending = None;
        }
        if let Some(pending) = self.pending.take() {
            let candidates = pending
                .candidates
                .into_iter()
                .filter(|&index| self.bindings[index].1.steps.get(pending.next) == Some(&combo))
                .collect::<Vec<_>>();
            if let Some(&complete) = candidates
                .iter()
                .find(|&&index| self.bindings[index].1.steps.len() == pending.next + 1)
            {
                return Some(self.bindings[complete].0.clone());
            }
            if !candidates.is_empty() {
                self.pending = Some(Pending {
                    candidates,
                    next: pending.next + 1,
                    deadline: now + self.timeout,
                });
                return None;
            }
        }

        let candidates = self
            .bindings
            .iter()
            .enumerate()
            .filter(|(_, (_, sequence))| {
                sequence.steps.len() > 1 && sequence.steps.first() == Some(&combo)
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        if !candidates.is_empty() {
            self.pending = Some(Pending {
                candidates,
                next: 1,
                deadline: now + self.timeout,
            });
            return None;
        }
        self.bindings
            .iter()
            .find(|(_, sequence)| sequence.steps.as_slice() == [combo])
            .map(|(action, _)| action.clone())
    }
}
