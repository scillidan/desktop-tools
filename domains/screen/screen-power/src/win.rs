//! Narrow Win32 window/monitor queries used only by monitor power recovery.
// shortcut: keep these queries local during the ownership move; consolidate
// them with other Win32 queries in the planned shared-kit cleanup.

use super::Saved;
use kit::layout::Rect;
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, MonitorFromWindow, HDC, HMONITOR, MONITORINFO,
    MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetAncestor, GetClassNameW, GetWindowLongW, GetWindowRect, IsIconic, IsWindow,
    IsWindowVisible, IsZoomed, SetWindowPos, ShowWindow, GA_ROOT, GWL_EXSTYLE, SWP_NOACTIVATE,
    SWP_NOZORDER, SW_MAXIMIZE, SW_NORMAL, WS_EX_TOOLWINDOW,
};

pub struct MonitorRow {
    pub handle: HMONITOR,
    pub work: Rect,
}

fn rect_of(r: RECT) -> Rect {
    Rect {
        l: r.left,
        t: r.top,
        r: r.right,
        b: r.bottom,
    }
}

pub fn enum_monitors() -> Vec<MonitorRow> {
    unsafe extern "system" fn cb(
        hmon: HMONITOR,
        _hdc: HDC,
        _rc: *mut RECT,
        lparam: LPARAM,
    ) -> BOOL {
        let rows = unsafe { &mut *(lparam.0 as *mut Vec<MonitorRow>) };
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if unsafe { GetMonitorInfoW(hmon, &mut info) }.as_bool() {
            rows.push(MonitorRow {
                handle: hmon,
                work: rect_of(info.rcWork),
            });
        }
        BOOL(1)
    }
    let mut rows = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(cb), LPARAM(&mut rows as *mut _ as isize));
    }
    rows
}

fn zonable(hwnd: HWND) -> bool {
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
        GetWindowLongW(hwnd, GWL_EXSTYLE) & WS_EX_TOOLWINDOW.0 as i32 == 0
    }
}

struct SnapCtx {
    mons: Vec<MonitorRow>,
    out: Vec<Saved>,
}

unsafe extern "system" fn snap_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let ctx = unsafe { &mut *(lparam.0 as *mut SnapCtx) };
    if !zonable(hwnd) {
        return BOOL(1);
    }
    let mut rect = RECT::default();
    unsafe {
        if GetWindowRect(hwnd, &mut rect).is_ok() {
            let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
            if let Some(row) = ctx.mons.iter().find(|row| row.handle == monitor) {
                ctx.out.push(Saved {
                    hwnd: hwnd.0 as isize,
                    work: row.work,
                    rect: rect_of(rect),
                    maximized: IsZoomed(hwnd).as_bool(),
                });
            }
        }
    }
    BOOL(1)
}

pub fn snapshot() -> Vec<Saved> {
    let mut ctx = SnapCtx {
        mons: enum_monitors(),
        out: Vec::new(),
    };
    unsafe {
        let _ = EnumWindows(Some(snap_cb), LPARAM(&mut ctx as *mut _ as isize));
    }
    ctx.out
}

pub fn needs_restore(saved_work: Rect, current_work: Option<Rect>, mons: &[MonitorRow]) -> bool {
    mons.iter().any(|m| m.work == saved_work) && current_work != Some(saved_work)
}

pub fn restore(saved: &[Saved], mons: &[MonitorRow]) {
    unsafe {
        for item in saved {
            let hwnd = HWND(item.hwnd as *mut core::ffi::c_void);
            if !IsWindow(Some(hwnd)).as_bool() || !zonable(hwnd) {
                continue;
            }
            let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
            let current = mons
                .iter()
                .find(|row| row.handle == monitor)
                .map(|row| row.work);
            if !needs_restore(item.work, current, mons) {
                continue;
            }
            if IsZoomed(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_NORMAL);
            }
            let _ = SetWindowPos(
                hwnd,
                None,
                item.rect.l,
                item.rect.t,
                item.rect.w(),
                item.rect.h(),
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            if item.maximized {
                let _ = ShowWindow(hwnd, SW_MAXIMIZE);
            }
        }
    }
}
