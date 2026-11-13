//! Ctrl+Alt+Z — fullscreen scale of the focused window via a DWM thumbnail.
//!
//! A real `SetWindowPos` zoom cannot paint a client area larger than its
//! monitor (DirectComposition clips at the target), so instead we register a
//! DWM thumbnail of the source HWND and show it on a click-through, layered,
//! topmost overlay stretched over the target monitor's *work area* (viewport,
//! not full monitor: taskbar stays visible, centering matches the monitor row).
//!
//! Screen Frame rendering is detected by its public window class, without a
//! dependency from Window back to Screen.

use kit::layout::Rect;
use std::mem::size_of;
use std::sync::atomic::{AtomicBool, Ordering};
use windows::core::PCWSTR;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromRect, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

static ACTIVE: AtomicBool = AtomicBool::new(false);
static mut OVERLAY: HWND = HWND(std::ptr::null_mut());

fn class_of(hwnd: HWND) -> String {
    unsafe {
        let mut buf = [0u16; 128];
        let len = GetClassNameW(hwnd, &mut buf);
        String::from_utf16_lossy(&buf[..len as usize])
    }
}

/// A Screen frame/overlay is a selection surface, not the app to zoom.
unsafe fn source_window() -> HWND {
    let foreground = GetForegroundWindow();
    if !class_of(foreground).starts_with("screen-tools.") {
        return foreground;
    }
    let mut point = POINT::default();
    if GetCursorPos(&mut point).is_err() {
        let pos = GetMessagePos();
        point = POINT {
            x: pos as i16 as i32,
            y: (pos >> 16) as i16 as i32,
        };
    }
    let mut candidate = GetTopWindow(None).unwrap_or_default();
    while !candidate.0.is_null() {
        let class = class_of(candidate);
        let ex = GetWindowLongPtrW(candidate, GWL_EXSTYLE) as u32;
        let mut rect = RECT::default();
        let inside = GetWindowRect(candidate, &mut rect).is_ok()
            && point.x >= rect.left
            && point.x < rect.right
            && point.y >= rect.top
            && point.y < rect.bottom;
        if inside
            && IsWindowVisible(candidate).as_bool()
            && !class.starts_with("screen-tools.")
            && !class.starts_with("window-tools.")
            && !matches!(
                class.as_str(),
                "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
            )
            && ex & (WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0) == 0
        {
            return GetAncestor(candidate, GA_ROOTOWNER);
        }
        candidate = GetWindow(candidate, GW_HWNDNEXT).unwrap_or_default();
    }
    HWND::default()
}

fn work_of(hwnd: HWND) -> Rect {
    unsafe {
        let mut rc = RECT::default();
        let _ = GetWindowRect(hwnd, &mut rc);
        let mon = MonitorFromRect(&rc, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(mon, &mut mi).as_bool() {
            Rect {
                l: mi.rcWork.left,
                t: mi.rcWork.top,
                r: mi.rcWork.right,
                b: mi.rcWork.bottom,
            }
        } else {
            Rect {
                l: rc.left,
                t: rc.top,
                r: rc.right,
                b: rc.bottom,
            }
        }
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_KEYDOWN => {
            stop();
            LRESULT(0)
        }
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

pub fn toggle() {
    unsafe {
        if ACTIVE.swap(true, Ordering::SeqCst) {
            stop();
            return;
        }
        let src = source_window();
        if src.0.is_null() {
            ACTIVE.store(false, Ordering::SeqCst);
            return;
        }
        let cls = windows::core::w!("window-tools.scale");
        let mut wc = WNDCLASSW::default();
        let hinst = GetModuleHandleW(None).unwrap_or_default().into();
        wc.lpfnWndProc = Some(wnd_proc);
        wc.hInstance = hinst;
        wc.lpszClassName = cls;
        RegisterClassW(&wc);
        let mut ex_style = WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW;
        // DWM thumbnail fails with DWM_E_NOT_COMPOSITED on a WS_EX_NOREDIRECTIONBITMAP window.
        if let Ok(frame) = FindWindowW(windows::core::w!("screen-tools.frame"), PCWSTR::null()) {
            if !frame.0.is_null() {
                ex_style |= WS_EX_NOREDIRECTIONBITMAP;
            }
        }
        let hwnd = match CreateWindowExW(
            ex_style,
            cls,
            windows::core::w!(""),
            WS_POPUP | WS_VISIBLE,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(hinst),
            None,
        ) {
            Ok(hwnd) => hwnd,
            Err(e) => {
                eprintln!("scale: CreateWindowExW failed: {e}");
                ACTIVE.store(false, Ordering::SeqCst);
                return;
            }
        };
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA);
        let thumb = match DwmRegisterThumbnail(hwnd, src) {
            Ok(thumb) => thumb,
            Err(e) => {
                eprintln!("scale: DwmRegisterThumbnail failed: {e}");
                let _ = DestroyWindow(hwnd);
                ACTIVE.store(false, Ordering::SeqCst);
                return;
            }
        };
        let target = work_of(hwnd);
        let props = DWM_THUMBNAIL_PROPERTIES {
            dwFlags: DWM_TNP_VISIBLE | DWM_TNP_RECTDESTINATION | DWM_TNP_OPACITY,
            rcDestination: RECT {
                left: 0,
                top: 0,
                right: target.w(),
                bottom: target.h(),
            },
            fVisible: true.into(),
            opacity: 255,
            ..Default::default()
        };
        let _ = DwmUpdateThumbnailProperties(thumb, &props);
        SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            target.l,
            target.t,
            target.w(),
            target.h(),
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        )
        .unwrap();
        OVERLAY = hwnd;
    }
}

unsafe fn stop() {
    let _ = DestroyWindow(OVERLAY);
    OVERLAY = HWND::default();
    ACTIVE.store(false, Ordering::SeqCst);
}
