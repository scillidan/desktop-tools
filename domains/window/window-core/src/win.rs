//! Small Win32 window primitives shared by window feature modules.

use crate::{Rect, WinId};
use anyhow::Result;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::WindowsAndMessaging::{
    GetAncestor, GetClassNameW, GetForegroundWindow, GetWindowLongW, GetWindowThreadProcessId,
    IsIconic, IsWindowVisible, SetForegroundWindow, GA_ROOT, GWL_EXSTYLE, WS_EX_TOOLWINDOW,
};

pub fn hwnd_of(id: WinId) -> HWND {
    HWND(id as *mut core::ffi::c_void)
}

pub fn id_of(hwnd: HWND) -> WinId {
    hwnd.0 as usize as u64
}

/// Bring `hwnd` to the foreground (shared with the mouse-drag hook).
pub fn focus_window(hwnd: HWND) -> Result<()> {
    unsafe {
        if SetForegroundWindow(hwnd).as_bool() {
            return Ok(());
        }
        // Foreground lock: attach to the current foreground thread's input
        // queue, retry, then detach.
        let fg = GetForegroundWindow();
        let fg_thread = GetWindowThreadProcessId(fg, None);
        let this = GetCurrentThreadId();
        let _ = AttachThreadInput(this, fg_thread, true);
        let ok = SetForegroundWindow(hwnd).as_bool();
        let _ = AttachThreadInput(this, fg_thread, false);
        anyhow::ensure!(ok, "SetForegroundWindow was refused");
    }
    Ok(())
}

/// Relaxed window eligibility shared by switcher, rectangle and drag gestures.
pub fn zonable(hwnd: HWND) -> bool {
    if hwnd.0.is_null() {
        return false;
    }
    unsafe {
        if GetAncestor(hwnd, GA_ROOT) != hwnd
            || !IsWindowVisible(hwnd).as_bool()
            || IsIconic(hwnd).as_bool()
        {
            return false;
        }
        let mut buf = [0u16; 256];
        let n = GetClassNameW(hwnd, &mut buf);
        if n == 0 {
            return false;
        }
        let class = String::from_utf16_lossy(&buf[..n as usize]);
        if matches!(
            class.as_str(),
            "Shell_TrayWnd"
                | "Shell_SecondaryTrayWnd"
                | "Progman"
                | "WorkerW"
                | "SysListView32"
                | "ForegroundStaging"
        ) {
            return false;
        }
        let ex = GetWindowLongW(hwnd, GWL_EXSTYLE);
        ex & WS_EX_TOOLWINDOW.0 as i32 == 0
    }
}

pub fn rect_of(r: RECT) -> Rect {
    Rect {
        l: r.left,
        t: r.top,
        r: r.right,
        b: r.bottom,
    }
}
