//! Exact-combo low-level keyboard hook for bindings that must swallow keys.
//! Injected events are ignored so SendInput pass-through and replacements do
//! not recursively trigger the hook.

use std::sync::atomic::{AtomicIsize, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};

use anyhow::{Context, Result};
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx, HC_ACTION, HHOOK,
    KBDLLHOOKSTRUCT, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

pub const WM_KBDHOOK: u32 = 0x8001;
const LLKHF_INJECTED: u32 = 0x10;
const LLKHF_LOWER_IL_INJECTED: u32 = 0x02;

static HOOK: AtomicIsize = AtomicIsize::new(0);
static THREAD_ID: AtomicU32 = AtomicU32::new(0);
static MESSAGE: AtomicU32 = AtomicU32::new(WM_KBDHOOK);
static ID_BASE: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Binding {
    pub vk: u8,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub win: bool,
    pub id: usize,
}

struct State {
    bindings: Vec<Binding>,
    down: [bool; 256],
}

static STATE: Mutex<State> = Mutex::new(State {
    bindings: Vec::new(),
    down: [false; 256],
});

fn state() -> MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn set_bindings(bindings: &[Binding]) {
    *state() = State {
        bindings: bindings.to_vec(),
        down: [false; 256],
    };
}

/// Install on the calling thread, which must pump messages. Each match posts
/// `message` with `wParam = id_base + binding.id` to the same thread.
pub fn install(message: u32, id_base: usize) -> Result<()> {
    if HOOK.load(Ordering::SeqCst) != 0 {
        return Ok(());
    }
    MESSAGE.store(message, Ordering::SeqCst);
    ID_BASE.store(id_base, Ordering::SeqCst);
    let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), None, 0) }
        .context("cannot install the low-level keyboard hook")?;
    HOOK.store(hook.0 as isize, Ordering::SeqCst);
    THREAD_ID.store(unsafe { GetCurrentThreadId() }, Ordering::SeqCst);
    Ok(())
}

pub fn uninstall() {
    let hook = HOOK.swap(0, Ordering::SeqCst);
    THREAD_ID.store(0, Ordering::SeqCst);
    if hook != 0 {
        unsafe {
            let _ = UnhookWindowsHookEx(HHOOK(hook as *mut core::ffi::c_void));
        }
    }
    state().bindings.clear();
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let kb = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let msg = wparam.0 as u32;
        if matches!(msg, WM_KEYDOWN | WM_SYSKEYDOWN | WM_KEYUP | WM_SYSKEYUP)
            && kb.flags.0 & (LLKHF_INJECTED | LLKHF_LOWER_IL_INJECTED) == 0
            && kb.vkCode < 0x100
        {
            let down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
            let mut st = state();
            if let Some(id) = on_key(&mut st, kb.vkCode as u8, down) {
                drop(st);
                mask_win_release();
                let tid = THREAD_ID.load(Ordering::SeqCst);
                if tid != 0 {
                    unsafe {
                        let _ = PostThreadMessageW(
                            tid,
                            MESSAGE.load(Ordering::SeqCst),
                            WPARAM(ID_BASE.load(Ordering::SeqCst) + id),
                            LPARAM(0),
                        );
                    }
                }
                return LRESULT(1);
            }
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn on_key(st: &mut State, vk: u8, down: bool) -> Option<usize> {
    st.down[vk as usize] = down;
    if !down {
        return None;
    }
    let (ctrl, shift, alt, win) = mods(&st.down);
    st.bindings
        .iter()
        .find(|b| b.vk == vk && b.ctrl == ctrl && b.shift == shift && b.alt == alt && b.win == win)
        .map(|b| b.id)
}

fn mods(down: &[bool; 256]) -> (bool, bool, bool, bool) {
    (
        down[0x11] || down[0xA2] || down[0xA3],
        down[0x10] || down[0xA0] || down[0xA1],
        down[0x12] || down[0xA4] || down[0xA5],
        down[0x5B] || down[0x5C],
    )
}

/// Mark Win as used so releasing it after a swallowed combo does not open Start.
fn mask_win_release() {
    for up in [false, true] {
        let input = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0x07),
                    dwFlags: if up {
                        KEYEVENTF_KEYUP
                    } else {
                        KEYBD_EVENT_FLAGS(0)
                    },
                    ..Default::default()
                },
            },
        };
        unsafe {
            let _ = SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding(vk: u8, id: usize) -> Binding {
        Binding {
            vk,
            ctrl: false,
            shift: false,
            alt: true,
            win: true,
            id,
        }
    }

    fn press(st: &mut State, vk: u8) -> Option<usize> {
        on_key(st, vk, true)
    }

    fn release(st: &mut State, vk: u8) {
        let _ = on_key(st, vk, false);
    }

    #[test]
    fn swallows_exact_combo_and_auto_repeat() {
        let mut st = State {
            bindings: vec![binding(0x31, 7)],
            down: [false; 256],
        };
        assert_eq!(press(&mut st, 0x5B), None);
        assert_eq!(press(&mut st, 0x12), None);
        assert_eq!(press(&mut st, 0x31), Some(7));
        assert_eq!(press(&mut st, 0x31), Some(7));
        release(&mut st, 0x31);
        release(&mut st, 0x12);
        release(&mut st, 0x5B);
        assert_eq!(press(&mut st, 0x31), None);
    }

    #[test]
    fn extra_or_released_modifier_prevents_the_match() {
        let mut st = State {
            bindings: vec![binding(0x31, 7)],
            down: [false; 256],
        };
        press(&mut st, 0x5B);
        press(&mut st, 0x12);
        press(&mut st, 0x10);
        assert_eq!(press(&mut st, 0x31), None);
        release(&mut st, 0x10);
        release(&mut st, 0x12);
        assert_eq!(press(&mut st, 0x31), None);
    }

    #[test]
    fn shift_combo_is_distinct() {
        let mut shifted = binding(0xC0, 1);
        shifted.shift = true;
        shifted.alt = false;
        let mut plain = binding(0xC0, 0);
        plain.alt = false;
        let mut st = State {
            bindings: vec![plain, shifted],
            down: [false; 256],
        };
        press(&mut st, 0x5B);
        assert_eq!(press(&mut st, 0xC0), Some(0));
        release(&mut st, 0xC0);
        press(&mut st, 0x10);
        assert_eq!(press(&mut st, 0xC0), Some(1));
    }
}
