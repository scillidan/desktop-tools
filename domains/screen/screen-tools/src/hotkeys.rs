//! Global hotkeys via RegisterHotKey (non-Win combos only — Win combos need
//! the kbdhook, which lands with the keyboard tools; see AGENTS.md).
//! Provenance: vk_of_key/modifiers_of adapted from desktop-tools-rs
//! src/win32.rs (repo folder window-tools-rs).

use crate::config::{Action, Bindings};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::*;

pub(crate) fn vk_of_key(key: kit::keys::Key) -> Option<VIRTUAL_KEY> {
    use kit::keys::Key;
    Some(match key {
        Key::Char('`') => VK_OEM_3,
        Key::Char(';') => VK_OEM_1,
        Key::Char('=') => VK_OEM_PLUS,
        Key::Char(',') => VK_OEM_COMMA,
        Key::Char('-') => VK_OEM_MINUS,
        Key::Char('.') => VK_OEM_PERIOD,
        Key::Char('/') => VK_OEM_2,
        Key::Char('[') => VK_OEM_4,
        Key::Char('\\') => VK_OEM_5,
        Key::Char(']') => VK_OEM_6,
        Key::Char('\'') => VK_OEM_7,
        Key::Char(c) => VIRTUAL_KEY(c.to_ascii_uppercase() as u16),
        Key::F(1) => VK_F1,
        Key::F(2) => VK_F2,
        Key::F(3) => VK_F3,
        Key::F(4) => VK_F4,
        Key::F(5) => VK_F5,
        Key::F(6) => VK_F6,
        Key::F(7) => VK_F7,
        Key::F(8) => VK_F8,
        Key::F(9) => VK_F9,
        Key::F(10) => VK_F10,
        Key::F(11) => VK_F11,
        Key::F(12) => VK_F12,
        Key::F(n) => VIRTUAL_KEY(VK_F12.0 + (n - 12) as u16),
        Key::Insert => VK_INSERT,
        Key::Delete => VK_DELETE,
        Key::Space => VK_SPACE,
        Key::Tab => VK_TAB,
        Key::Backspace => VK_BACK,
        Key::Escape => VK_ESCAPE,
        Key::Enter => VK_RETURN,
        Key::Left => VK_LEFT,
        Key::Right => VK_RIGHT,
        Key::Up => VK_UP,
        Key::Down => VK_DOWN,
        Key::Home => VK_HOME,
        Key::End => VK_END,
        Key::PageUp => VK_PRIOR,
        Key::PageDown => VK_NEXT,
        Key::PrintScreen => VK_SNAPSHOT,
        Key::Pause => VK_PAUSE,
        Key::NumpadDot | Key::CapsLock | Key::Other => return None,
    })
}

fn modifiers_of(c: &kit::keys::Combo) -> HOT_KEY_MODIFIERS {
    let mut m = MOD_NOREPEAT;
    if c.ctrl {
        m |= MOD_CONTROL;
    }
    if c.shift {
        m |= MOD_SHIFT;
    }
    if c.alt {
        m |= MOD_ALT;
    }
    if c.win {
        m |= MOD_WIN;
    }
    m
}

fn modifier_down(vk: VIRTUAL_KEY) -> bool {
    unsafe {
        GetAsyncKeyState(vk.0 as i32) < 0
            || (vk == VK_LWIN && GetAsyncKeyState(VK_RWIN.0 as i32) < 0)
    }
}

fn send_combo(combo: &kit::keys::Combo, key: VIRTUAL_KEY) -> bool {
    let configured = [
        (combo.ctrl, VK_CONTROL),
        (combo.shift, VK_SHIFT),
        (combo.alt, VK_MENU),
        (combo.win, VK_LWIN),
    ];
    // A very quick tap can queue the original modifier release before the
    // reinjection. Let the original sequence drain before choosing which
    // modifiers need to be synthesized.
    std::thread::sleep(std::time::Duration::from_millis(20));
    let mut synthetic = Vec::new();
    let mut inputs = Vec::with_capacity(10);
    let mut press = |vk: VIRTUAL_KEY, up: bool| {
        inputs.push(INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    dwFlags: if up {
                        KEYEVENTF_KEYUP
                    } else {
                        KEYBD_EVENT_FLAGS(0)
                    },
                    ..Default::default()
                },
            },
        });
    };
    for (enabled, vk) in configured {
        // Preserve modifiers the user is still holding instead of sending a
        // synthetic release for a physical key.
        if enabled && !modifier_down(vk) {
            press(vk, false);
            synthetic.push(vk);
        }
    }
    press(key, false);
    press(key, true);
    for vk in synthetic.into_iter().rev() {
        press(vk, true);
    }
    unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) as usize == inputs.len() }
}

/// Registered hotkey set. Ids are 1-based indexes into the registered actions.
pub struct Hotkeys {
    combos: Vec<kit::keys::Combo>,
    actions: Vec<Action>,
}

impl Hotkeys {
    /// Registers every binding against `hwnd` (window-tied so hotkey
    /// detective tools can attribute the owner; a thread-tied registration
    /// works the same but shows no owner). Returns the combos that could
    /// not be registered (conflict or unsupported key); the caller warns
    /// and runs with the rest, per the desktop-tools-rs convention.
    pub fn register(hwnd: HWND, bindings: &Bindings) -> (Hotkeys, Vec<String>) {
        Self::register_selected(hwnd, bindings, false)
    }

    /// Power remains available while Flameshot owns every other Screen chord.
    pub fn register_for_handoff(hwnd: HWND, bindings: &Bindings) -> (Hotkeys, Vec<String>) {
        Self::register_selected(hwnd, bindings, true)
    }

    fn register_selected(
        hwnd: HWND,
        bindings: &Bindings,
        handoff_only: bool,
    ) -> (Hotkeys, Vec<String>) {
        let mut combos = Vec::new();
        let mut actions = Vec::new();
        let mut failed = Vec::new();
        for action in Action::ALL {
            if handoff_only && action != Action::ToggleScreen {
                continue;
            }
            let combo = action.get(bindings);
            match vk_of_key(combo.key) {
                Some(vk) => {
                    let id = (combos.len() + 1) as i32;
                    let ok = unsafe {
                        RegisterHotKey(Some(hwnd), id, modifiers_of(&combo), vk.0 as u32).is_ok()
                    };
                    if ok {
                        combos.push(combo);
                        actions.push(action);
                    } else {
                        failed.push(format!("{} = \"{}\"", action.name(), combo));
                    }
                }
                None => failed.push(format!("{} = \"{}\" (unknown key)", action.name(), combo)),
            }
        }
        (Hotkeys { combos, actions }, failed)
    }

    pub fn unregister_for_handoff(&self, hwnd: HWND) {
        for (i, action) in self.actions.iter().enumerate() {
            if *action != Action::ToggleScreen {
                unsafe {
                    let _ = UnregisterHotKey(Some(hwnd), (i + 1) as i32);
                }
            }
        }
    }

    pub fn unregister_all(&self, hwnd: HWND) {
        for (i, _) in self.combos.iter().enumerate() {
            unsafe {
                let _ = UnregisterHotKey(Some(hwnd), (i + 1) as i32);
            }
        }
    }

    /// Maps a WM_HOTKEY id back to its action.
    pub fn action_of(&self, id: i32) -> Option<Action> {
        if id < 1 || id as usize > self.combos.len() {
            return None;
        }
        self.actions.get((id - 1) as usize).copied()
    }

    /// Return a chord Screen cannot use to the foreground application.
    /// Unregister first so the injected input does not recursively invoke us.
    pub fn pass_through(&self, hwnd: HWND, id: i32) -> bool {
        let Some(combo) = id
            .checked_sub(1)
            .and_then(|index| self.combos.get(index as usize))
        else {
            return false;
        };
        let Some(vk) = vk_of_key(combo.key) else {
            return false;
        };
        if unsafe { UnregisterHotKey(Some(hwnd), id) }.is_err() {
            return false;
        }
        let sent = send_combo(combo, vk);
        let restored =
            unsafe { RegisterHotKey(Some(hwnd), id, modifiers_of(combo), vk.0 as u32).is_ok() };
        sent && restored
    }

    pub fn is_empty(&self) -> bool {
        self.combos.is_empty()
    }

    pub fn len(&self) -> usize {
        self.combos.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kit::keys::Key;

    #[test]
    fn sparse_registration_preserves_action_ids() {
        let hotkeys = Hotkeys {
            combos: vec!["win+f2".parse().unwrap()],
            actions: vec![Action::ToggleScreen],
        };
        assert_eq!(hotkeys.action_of(1), Some(Action::ToggleScreen));
    }

    #[test]
    fn vk_mapping_covers_all_combo_keys() {
        // Every Key variant a binding can parse must map to a VK code,
        // otherwise register() silently fails the binding.
        for key in [
            Key::Char('a'),
            Key::Char('`'),
            Key::Char('['),
            Key::F(13),
            Key::Enter,
            Key::PrintScreen,
        ] {
            assert!(vk_of_key(key).is_some(), "{key:?} has no VK mapping");
        }
        assert!(vk_of_key(Key::Other).is_none());
    }
}
