//! Bounded, immediate trigger recognition with no timers or clipboard.

use anyhow::{bail, Result};
use std::collections::BTreeMap;

const MAX_MAPPINGS: usize = 4096;
const MAX_TRIGGER_CHARS: usize = 128;
const MAX_REPLACEMENT_UTF16: usize = 8192;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expansion {
    pub backspaces: usize,
    pub replacement: String,
}

#[derive(Debug)]
pub struct Engine {
    triggers: BTreeMap<Vec<char>, String>,
    history: Vec<char>,
    max_trigger: usize,
}

impl Engine {
    pub fn new(mappings: BTreeMap<String, String>) -> Result<Self> {
        if mappings.len() > MAX_MAPPINGS {
            bail!("keymap contains more than {MAX_MAPPINGS} mappings");
        }
        let mut triggers = BTreeMap::new();
        for (trigger, replacement) in mappings {
            let chars: Vec<char> = trigger.chars().collect();
            if chars.is_empty() || chars.len() > MAX_TRIGGER_CHARS {
                bail!(
                    "keymap trigger must contain 1..={MAX_TRIGGER_CHARS} characters: {trigger:?}"
                );
            }
            if chars.iter().any(|c| c.is_control()) {
                bail!("keymap trigger cannot contain control characters: {trigger:?}");
            }
            if replacement.encode_utf16().count() > MAX_REPLACEMENT_UTF16 {
                bail!("keymap replacement is too long for trigger {trigger:?}");
            }
            triggers.insert(chars, replacement);
        }
        let keys: Vec<&Vec<char>> = triggers.keys().collect();
        for (index, left) in keys.iter().enumerate() {
            for right in keys.iter().skip(index + 1) {
                if is_prefix(left, right)
                    || is_prefix(right, left)
                    || is_suffix(left, right)
                    || is_suffix(right, left)
                {
                    bail!(
                        "ambiguous immediate keymap triggers {:?} and {:?}",
                        left.iter().collect::<String>(),
                        right.iter().collect::<String>()
                    );
                }
            }
        }
        let max_trigger = triggers.keys().map(Vec::len).max().unwrap_or(1);
        Ok(Self {
            triggers,
            history: Vec::with_capacity(max_trigger),
            max_trigger,
        })
    }

    pub fn push(&mut self, ch: char) -> Option<Expansion> {
        self.history.push(ch);
        if self.history.len() > self.max_trigger {
            self.history.remove(0);
        }
        let (trigger, replacement) = self
            .triggers
            .iter()
            .filter(|(trigger, _)| is_suffix(trigger, &self.history))
            .max_by_key(|(trigger, _)| trigger.len())?;
        let expansion = Expansion {
            backspaces: trigger.len() - 1,
            replacement: replacement.clone(),
        };
        self.history.clear();
        Some(expansion)
    }

    pub fn backspace(&mut self) {
        self.history.pop();
    }

    pub fn reset(&mut self) {
        self.history.clear();
    }
}

fn is_prefix(left: &[char], right: &[char]) -> bool {
    left.len() < right.len() && right.starts_with(left)
}

fn is_suffix(left: &[char], right: &[char]) -> bool {
    left.len() <= right.len() && right.ends_with(left)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine(entries: &[(&str, &str)]) -> Engine {
        Engine::new(
            entries
                .iter()
                .map(|(trigger, replacement)| (trigger.to_string(), replacement.to_string()))
                .collect(),
        )
        .unwrap()
    }

    fn type_text(engine: &mut Engine, text: &str) -> Option<Expansion> {
        text.chars().find_map(|ch| engine.push(ch))
    }

    #[test]
    fn replaces_exact_case_sensitive_trigger_without_an_end_character() {
        let mut engine = engine(&[("#alpha", "α"), ("#Alpha", "Α")]);
        assert_eq!(
            type_text(&mut engine, "#alpha"),
            Some(Expansion {
                backspaces: 5,
                replacement: "α".into()
            })
        );
        assert_eq!(engine.push('.'), None);
        assert_eq!(
            type_text(&mut engine, "#Alpha"),
            Some(Expansion {
                backspaces: 5,
                replacement: "Α".into()
            })
        );
    }

    #[test]
    fn arbitrary_prefix_and_empty_replacement_work() {
        let mut engine = engine(&[(">>star", "★"), ("#->", "→"), (";em", "")]);
        assert_eq!(type_text(&mut engine, ">>star").unwrap().replacement, "★");
        assert_eq!(type_text(&mut engine, "#->").unwrap().backspaces, 2);
        assert_eq!(type_text(&mut engine, ";em").unwrap().replacement, "");
    }

    #[test]
    fn replacement_does_not_reenter_the_recognizer() {
        let mut engine = engine(&[("#a", "#b"), ("#b", "done")]);
        assert_eq!(type_text(&mut engine, "#a").unwrap().replacement, "#b");
        assert_eq!(type_text(&mut engine, "x#b").unwrap().replacement, "done");
    }

    #[test]
    fn backspace_restores_a_prefix_before_the_next_character() {
        let mut engine = engine(&[("#ab", "ok")]);
        type_text(&mut engine, "#ax");
        engine.backspace();
        assert_eq!(type_text(&mut engine, "b").unwrap().replacement, "ok");
    }

    #[test]
    fn reset_and_bounded_history_leave_no_stuck_state() {
        let mut engine = engine(&[("#ab", "ok")]);
        engine.push('#');
        engine.reset();
        assert_eq!(type_text(&mut engine, "ab"), None);
        for _ in 0..1000 {
            engine.push('x');
        }
        assert!(engine.history.len() <= 3);
    }

    #[test]
    fn rejects_prefix_suffix_and_invalid_triggers() {
        assert!(engine_result(&[("#a", "a"), ("#ab", "b")]).is_err());
        assert!(engine_result(&[("ab", "a"), ("cab", "b")]).is_err());
        assert!(engine_result(&[("", "empty")]).is_err());
        assert!(engine_result(&[("a\nb", "newline")]).is_err());
    }

    fn engine_result(entries: &[(&str, &str)]) -> Result<Engine> {
        Engine::new(
            entries
                .iter()
                .map(|(trigger, replacement)| (trigger.to_string(), replacement.to_string()))
                .collect(),
        )
    }
}
