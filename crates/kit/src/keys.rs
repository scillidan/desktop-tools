//! Key and key-combo primitives shared by all apps.
//!
//! Combo strings follow the keymap-rs convention: `ctrl+shift+c`, `altgr+shift+w`,
//! `win+f2`, `ctrl+alt+shift+insert`.

use anyhow::{bail, Result};
use std::fmt;
use std::str::FromStr;

/// A key identity. Letters and digits carry an ASCII char; function keys F1-F24.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Key {
    Char(char),
    F(u8),
    NumpadDot,
    Insert,
    Delete,
    Space,
    Tab,
    CapsLock,
    PrintScreen,
    Pause,
    Backspace,
    Enter,
    Escape,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Other,
}

/// A modifier+key combination like `altgr+shift+w` or `ctrl+shift+c`.
/// `altgr` is exclusive with `ctrl`/`alt` when matching: AltGr is reported by
/// backends as RAlt, which on Windows arrives as LCtrl+RAlt, but logically it
/// is one modifier.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Combo {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub altgr: bool,
    pub win: bool,
    pub key: Key,
}

impl Default for Combo {
    fn default() -> Self {
        Combo {
            ctrl: false,
            shift: false,
            alt: false,
            altgr: false,
            win: false,
            key: Key::Other,
        }
    }
}

impl Combo {
    /// Canonicalize: AltGr absorbs ctrl/alt, so `ctrl+alt+a` typed on an
    /// AltGr-ish chord compares equal to `altgr+a`.
    pub fn canonical(mut self) -> Combo {
        if self.altgr {
            self.ctrl = false;
            self.alt = false;
        }
        self
    }
}

impl FromStr for Combo {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Combo> {
        if s.trim_start()
            .chars()
            .next()
            .is_some_and(|ch| matches!(ch, '^' | '!' | '+' | '#'))
        {
            bail!("combo `{s}` uses unsupported sigils; use ctrl/alt/shift/win names");
        }

        let mut c = Combo::default();
        let mut key_name: Option<String> = None;
        for part in s.split('+') {
            let part = part.trim().to_ascii_lowercase();
            match part.as_str() {
                "" => continue,
                "ctrl" | "control" => c.ctrl = true,
                "shift" => c.shift = true,
                "alt" => c.alt = true,
                "altgr" | "ralt" => c.altgr = true,
                "win" | "meta" => c.win = true,
                name => {
                    if key_name.is_some() {
                        bail!("combo `{s}` has more than one non-modifier key");
                    }
                    key_name = Some(name.to_string());
                }
            }
        }
        let name = key_name.ok_or_else(|| anyhow::anyhow!("combo `{s}` is missing a key"))?;
        c.key = parse_key(&name)
            .ok_or_else(|| anyhow::anyhow!("unknown key `{name}` in combo `{s}`"))?;
        Ok(c.canonical())
    }
}

fn parse_key(name: &str) -> Option<Key> {
    let b = name.as_bytes();
    if b.len() == 1 && b[0].is_ascii_graphic() {
        return Some(Key::Char(b[0].to_ascii_lowercase() as char));
    }
    if let Some(rest) = name.strip_prefix('f') {
        if let Ok(n) = rest.parse::<u8>() {
            if (1..=24).contains(&n) {
                return Some(Key::F(n));
            }
        }
        return None;
    }
    match name {
        "numpaddot" | "numpad_dot" | "decimal" | "numpaddecimal" => Some(Key::NumpadDot),
        "insert" | "ins" => Some(Key::Insert),
        "delete" | "del" => Some(Key::Delete),
        "space" => Some(Key::Space),
        "tab" => Some(Key::Tab),
        "capslock" | "caps" => Some(Key::CapsLock),
        "printscreen" | "printscr" | "prtsc" | "prtscn" => Some(Key::PrintScreen),
        "pause" => Some(Key::Pause),
        "backspace" => Some(Key::Backspace),
        "enter" | "return" => Some(Key::Enter),
        "escape" | "esc" => Some(Key::Escape),
        "left" => Some(Key::Left),
        "right" => Some(Key::Right),
        "up" => Some(Key::Up),
        "down" => Some(Key::Down),
        "home" => Some(Key::Home),
        "end" => Some(Key::End),
        "pageup" => Some(Key::PageUp),
        "pagedown" => Some(Key::PageDown),
        _ => None,
    }
}

impl fmt::Display for Combo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let c = self.canonical();
        let mut parts: Vec<String> = Vec::new();
        if c.ctrl {
            parts.push("ctrl".into());
        }
        if c.shift {
            parts.push("shift".into());
        }
        if c.alt {
            parts.push("alt".into());
        }
        if c.altgr {
            parts.push("altgr".into());
        }
        if c.win {
            parts.push("win".into());
        }
        parts.push(match c.key {
            Key::Char(ch) => ch.to_string(),
            Key::F(n) => format!("f{n}"),
            Key::NumpadDot => "numpaddot".into(),
            Key::Insert => "insert".into(),
            Key::Delete => "delete".into(),
            Key::Space => "space".into(),
            Key::Tab => "tab".into(),
            Key::CapsLock => "capslock".into(),
            Key::PrintScreen => "printscreen".into(),
            Key::Pause => "pause".into(),
            Key::Backspace => "backspace".into(),
            Key::Enter => "enter".into(),
            Key::Escape => "escape".into(),
            Key::Left => "left".into(),
            Key::Right => "right".into(),
            Key::Up => "up".into(),
            Key::Down => "down".into(),
            Key::Home => "home".into(),
            Key::End => "end".into(),
            Key::PageUp => "pageup".into(),
            Key::PageDown => "pagedown".into(),
            Key::Other => "?".into(),
        });
        write!(f, "{}", parts.join("+"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_altgr_shift() {
        let c: Combo = "altgr+shift+w".parse().unwrap();
        assert!(c.altgr && c.shift && !c.ctrl && !c.alt);
        assert_eq!(c.key, Key::Char('w'));
    }

    #[test]
    fn parse_ctrl_shift_c() {
        let c: Combo = "Ctrl+Shift+C".parse().unwrap();
        assert!(c.ctrl && c.shift && !c.altgr);
        assert_eq!(c.key, Key::Char('c'));
    }

    #[test]
    fn parse_digit_keys() {
        let c: Combo = "alt+1".parse().unwrap();
        assert!(c.alt && c.key == Key::Char('1'));
        let c: Combo = "win+5".parse().unwrap();
        assert!(c.win && c.key == Key::Char('5'));
    }

    #[test]
    fn parse_function_keys() {
        let c: Combo = "win+f2".parse().unwrap();
        assert!(c.win && c.key == Key::F(2));
        let c: Combo = "ctrl+alt+shift+insert".parse().unwrap();
        assert!(c.ctrl && c.alt && c.shift && c.key == Key::Insert);
        let c: Combo = "capslock".parse().unwrap();
        assert_eq!(c.key, Key::CapsLock);
        assert!("f25".parse::<Combo>().is_err());
    }

    #[test]
    fn display_roundtrip() {
        for s in [
            "altgr+shift+w",
            "ctrl+shift+c",
            "win+c",
            "win+f2",
            "ctrl+alt+shift+insert",
            "capslock",
        ] {
            let c: Combo = s.parse().unwrap();
            let displayed = c.to_string();
            let c2: Combo = displayed.parse().unwrap();
            assert_eq!(c, c2, "`{s}` displayed as `{displayed}`");
        }
    }

    #[test]
    fn reject_sigil_modifiers() {
        for legacy in ["!+z", "^!+g", "#f2", "+z"] {
            assert!(legacy.parse::<Combo>().is_err(), "accepted `{legacy}`");
        }
    }

    #[test]
    fn reject_double_key() {
        assert!("ctrl+a+b".parse::<Combo>().is_err());
        assert!("ctrl+".parse::<Combo>().is_err());
    }
}
