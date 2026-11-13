//! Monitor power toggle with multi-monitor window restoration.
//!
//! DisplayPort power-off can make Windows treat a monitor as unplugged and
//! consolidate its windows. Snapshot their monitor work areas and rectangles
//! before power-off, then restore them after the disconnected monitor returns.

mod win;

use kit::layout::Rect;
use std::sync::{Mutex, MutexGuard};
use win::enum_monitors;
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW, WM_SYSCOMMAND};

const SC_MONITORPOWER: usize = 0xF170;

struct Saved {
    hwnd: isize,
    work: Rect,
    rect: Rect,
    maximized: bool,
}

struct State {
    forced_off: bool,
    expected: usize,
    disconnected: bool,
    saved: Vec<Saved>,
}

static STATE: Mutex<State> = Mutex::new(State {
    forced_off: false,
    expected: 0,
    disconnected: false,
    saved: Vec::new(),
});

fn state() -> MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Toggle monitor power. While forced off, `tick` reasserts power-off so stray
/// input cannot leave the monitors awake.
pub fn toggle() {
    let mut st = state();
    if st.forced_off {
        st.forced_off = false;
        st.saved.clear();
        power(-1);
    } else {
        st.saved = win::snapshot();
        st.expected = enum_monitors().len();
        st.disconnected = false;
        st.forced_off = true;
        power(2);
    }
}

/// Call from the owning process's one-second timer.
pub fn tick() {
    let mut st = state();
    if st.forced_off {
        power(2);
    }
    if st.saved.is_empty() {
        return;
    }
    let mons = enum_monitors();
    if mons.len() < st.expected {
        st.disconnected = true;
        return;
    }
    if st.disconnected {
        win::restore(&st.saved, &mons);
        st.saved.clear();
    }
}

fn power(mode: isize) {
    unsafe {
        if let Ok(progman) = FindWindowW(windows::core::w!("Progman"), None) {
            if !progman.0.is_null() {
                let _ = PostMessageW(
                    Some(progman),
                    WM_SYSCOMMAND,
                    WPARAM(SC_MONITORPOWER),
                    LPARAM(mode),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::win::MonitorRow;
    use windows::Win32::Graphics::Gdi::HMONITOR;

    fn mon(work: Rect) -> MonitorRow {
        MonitorRow {
            handle: HMONITOR(std::ptr::null_mut()),
            work,
        }
    }

    #[test]
    fn restore_only_when_the_monitor_is_back_and_the_window_left_it() {
        let primary = Rect {
            l: 0,
            t: 0,
            r: 1000,
            b: 700,
        };
        let secondary = Rect {
            l: 1000,
            t: 0,
            r: 2000,
            b: 700,
        };
        let both = vec![mon(primary), mon(secondary)];

        assert!(!win::needs_restore(secondary, Some(primary), &both[..1]));
        assert!(win::needs_restore(secondary, Some(primary), &both));
        assert!(!win::needs_restore(secondary, Some(secondary), &both));
        assert!(!win::needs_restore(secondary, Some(primary), &both[..1]));
    }
}
