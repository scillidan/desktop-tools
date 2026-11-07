//! Physical-key hook. The pure trigger machine lives in `engine`.

use crate::engine::{Engine, Expansion};
use anyhow::{bail, Context, Result};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetKeyboardLayout, GetKeyboardState, SendInput, ToUnicodeEx, INPUT, INPUT_0,
    INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, VIRTUAL_KEY,
    VK_BACK, VK_CONTROL, VK_LCONTROL, VK_LMENU, VK_LWIN, VK_MENU, VK_RCONTROL, VK_RMENU, VK_RWIN,
    VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetForegroundWindow, GetWindowThreadProcessId, PostThreadMessageW,
    SetWindowsHookExW, UnhookWindowsHookEx, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT, WH_KEYBOARD_LL,
    WH_MOUSE_LL, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_RBUTTONDOWN,
    WM_SYSKEYDOWN, WM_SYSKEYUP, WM_XBUTTONDOWN,
};

pub const WM_KEYMAP_ERROR: usize = 0x8000 + 42;
const LLKHF_INJECTED: u32 = 0x10;
const LLKHF_LOWER_IL_INJECTED: u32 = 0x02;
const TO_UNICODE_PRESERVE_DEAD_KEYS: u32 = 4;

static THREAD_ID: AtomicU32 = AtomicU32::new(0);
static STATE: Mutex<Option<HookState>> = Mutex::new(None);
static ERROR: Mutex<Option<String>> = Mutex::new(None);

struct HookState {
    engine: Engine,
    down: [bool; 256],
    swallowed: Option<u8>,
    focus: isize,
}

pub struct HookGuard {
    keyboard: HHOOK,
    mouse: HHOOK,
}

impl HookGuard {
    pub fn install(engine: Engine) -> Result<Self> {
        if state().is_some() {
            bail!("keymap hook is already installed");
        }
        *state() = Some(HookState {
            engine,
            down: [false; 256],
            swallowed: None,
            focus: 0,
        });
        let (keyboard, mouse) = match install_handles() {
            Ok(handles) => handles,
            Err(e) => {
                *state() = None;
                return Err(e);
            }
        };
        THREAD_ID.store(unsafe { GetCurrentThreadId() }, Ordering::SeqCst);
        Ok(Self { keyboard, mouse })
    }

    /// Install new hooks before removing the old ones, so a failed reload
    /// leaves the previous valid recognizer active.
    pub fn replace(&mut self, engine: Engine) -> Result<()> {
        let (keyboard, mouse) = install_handles()?;
        *state() = Some(HookState {
            engine,
            down: [false; 256],
            swallowed: None,
            focus: 0,
        });
        unsafe {
            let _ = UnhookWindowsHookEx(self.keyboard);
            let _ = UnhookWindowsHookEx(self.mouse);
        }
        self.keyboard = keyboard;
        self.mouse = mouse;
        Ok(())
    }
}

impl Drop for HookGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = UnhookWindowsHookEx(self.keyboard);
            let _ = UnhookWindowsHookEx(self.mouse);
        }
        THREAD_ID.store(0, Ordering::SeqCst);
        *state() = None;
    }
}

fn install_handles() -> Result<(HHOOK, HHOOK)> {
    let keyboard = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), None, 0) }
        .context("cannot install Key Map keyboard hook")?;
    match unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), None, 0) } {
        Ok(mouse) => Ok((keyboard, mouse)),
        Err(e) => {
            unsafe {
                let _ = UnhookWindowsHookEx(keyboard);
            }
            Err(e).context("cannot install Key Map mouse-reset hook")
        }
    }
}

pub fn take_error() -> Option<String> {
    ERROR.lock().unwrap_or_else(|e| e.into_inner()).take()
}

fn state() -> MutexGuard<'static, Option<HookState>> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let kb = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let msg = wparam.0 as u32;
        if matches!(msg, WM_KEYDOWN | WM_SYSKEYDOWN | WM_KEYUP | WM_SYSKEYUP)
            && kb.flags.0 & (LLKHF_INJECTED | LLKHF_LOWER_IL_INJECTED) == 0
            && kb.vkCode < 0x100
            && handle_key(kb, msg)
        {
            return LRESULT(1);
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32
        && matches!(
            wparam.0 as u32,
            WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN
        )
    {
        if let Some(st) = state().as_mut() {
            st.engine.reset();
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn handle_key(kb: &KBDLLHOOKSTRUCT, msg: u32) -> bool {
    let vk = kb.vkCode as u8;
    let down = matches!(msg, WM_KEYDOWN | WM_SYSKEYDOWN);
    let mut guard = state();
    let Some(st) = guard.as_mut() else {
        return false;
    };
    st.down[vk as usize] = down;

    if !down {
        if st.swallowed == Some(vk) {
            st.swallowed = None;
            return true;
        }
        return false;
    }
    if st.swallowed == Some(vk) {
        return true; // Do not repeat an expansion during key auto-repeat.
    }

    let foreground = unsafe { GetForegroundWindow() };
    let focus = foreground.0 as isize;
    if st.focus != focus {
        st.focus = focus;
        st.engine.reset();
    }
    if vk == VK_BACK.0 as u8 {
        st.engine.backspace();
        return false;
    }
    if is_shift(vk) {
        return false;
    }
    if is_modifier(vk) {
        st.engine.reset();
        return false;
    }

    let thread = unsafe { GetWindowThreadProcessId(HWND(focus as *mut core::ffi::c_void), None) };
    let layout = unsafe { GetKeyboardLayout(thread) };
    let mut keys = [0u8; 256];
    if unsafe { GetKeyboardState(&mut keys) }.is_err() {
        st.engine.reset();
        return false;
    }
    apply_tracked_modifiers(&mut keys, &st.down);
    keys[vk as usize] |= 0x80;
    let mut buffer = [0u16; 8];
    let count = unsafe {
        ToUnicodeEx(
            vk as u32,
            kb.scanCode,
            &keys,
            &mut buffer,
            TO_UNICODE_PRESERVE_DEAD_KEYS,
            Some(layout),
        )
    };
    let text = if count > 0 {
        String::from_utf16(&buffer[..count as usize]).ok()
    } else {
        None
    };
    let mut chars = text.as_deref().unwrap_or("").chars();
    let (Some(ch), None) = (chars.next(), chars.next()) else {
        st.engine.reset(); // Dead keys, IME and multi-character keys pass through.
        return false;
    };
    if ch.is_control() {
        st.engine.reset();
        return false;
    }
    if let Some(expansion) = st.engine.push(ch) {
        st.swallowed = Some(vk);
        drop(guard);
        if let Err(e) = expand(expansion) {
            *ERROR.lock().unwrap_or_else(|e| e.into_inner()) = Some(format!("{e:#}"));
            let thread = THREAD_ID.load(Ordering::SeqCst);
            if thread != 0 {
                unsafe {
                    let _ =
                        PostThreadMessageW(thread, WM_KEYMAP_ERROR as u32, WPARAM(0), LPARAM(0));
                }
            }
        }
        return true;
    }
    false
}

fn apply_tracked_modifiers(keys: &mut [u8; 256], down: &[bool; 256]) {
    for (index, is_down) in down.iter().enumerate() {
        if *is_down {
            keys[index] |= 0x80;
        }
    }
    // Low-level hooks report left/right keys, while ToUnicodeEx also reads the
    // generic modifier entries. Without this, Shift+3 is translated as `3`.
    for (generic, left, right) in [(0x10, 0xA0, 0xA1), (0x11, 0xA2, 0xA3), (0x12, 0xA4, 0xA5)] {
        if down[left] || down[right] {
            keys[generic] |= 0x80;
        }
    }
}

fn is_shift(vk: u8) -> bool {
    vk == VK_SHIFT.0 as u8 || vk == 0xA0 || vk == 0xA1
}

fn is_modifier(vk: u8) -> bool {
    vk == VK_CONTROL.0 as u8
        || vk == VK_MENU.0 as u8
        || matches!(vk, 0xA2 | 0xA3 | 0xA4 | 0xA5 | 0x5B | 0x5C)
}

fn expand(expansion: Expansion) -> Result<()> {
    let mut inputs = Vec::new();
    let mut restore = Vec::new();
    for vk in [
        VK_LCONTROL,
        VK_RCONTROL,
        VK_LMENU,
        VK_RMENU,
        VK_LWIN,
        VK_RWIN,
    ] {
        if unsafe { GetAsyncKeyState(vk.0 as i32) } < 0 {
            inputs.push(key(vk, true));
            restore.push(vk);
        }
    }
    for _ in 0..expansion.backspaces {
        inputs.push(key(VK_BACK, false));
        inputs.push(key(VK_BACK, true));
    }
    for unit in expansion.replacement.encode_utf16() {
        inputs.push(unicode(unit, false));
        inputs.push(unicode(unit, true));
    }
    inputs.extend(restore.into_iter().map(|vk| key(vk, false)));
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) } as usize;
    if sent != inputs.len() {
        bail!(
            "SendInput delivered {sent} of {} Key Map events",
            inputs.len()
        );
    }
    Ok(())
}

fn key(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    INPUT {
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
    }
}

fn unicode(unit: u16, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: unit,
                dwFlags: if up {
                    KEYEVENTF_UNICODE | KEYEVENTF_KEYUP
                } else {
                    KEYEVENTF_UNICODE
                },
                ..Default::default()
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::apply_tracked_modifiers;

    #[test]
    fn left_and_right_modifiers_populate_generic_to_unicode_state() {
        let mut down = [false; 256];
        down[0xA0] = true;
        down[0xA3] = true;
        down[0xA5] = true;
        let mut keys = [0u8; 256];
        apply_tracked_modifiers(&mut keys, &down);
        assert_eq!(keys[0x10] & 0x80, 0x80);
        assert_eq!(keys[0x11] & 0x80, 0x80);
        assert_eq!(keys[0x12] & 0x80, 0x80);
    }
}
