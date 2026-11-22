//! Capture/frame/pin coordination. Pixel and clipboard operations live in
//! `screen-capture`; this facade owns transitions that involve Frame or Pin.

use crate::logline;
use anyhow::Result;
use screen_capture::grab;
pub use screen_capture::{
    capture_frame, choose_file, copy_text, default_capture_name, target_window, window_target,
};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowRect, SetWindowPos, ShowWindow, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SWP_SHOWWINDOW, SW_HIDE, SW_SHOWNOACTIVATE,
};

/// Puts an explicit rect on screen as a pinned image; no clipboard, no file.
/// # Safety
/// The caller must provide valid screen rectangles and frame HWNDs on the owning UI thread.
pub unsafe fn pin_rect(rect: RECT, style: crate::pin::Style, owner: HWND) -> Result<()> {
    let shot = grab(rect)?;
    let (w, h) = (shot.w, shot.h);
    let data = shot.bgra();
    crate::pin::show(rect, w, h, &data, style, owner)
}

/// Same, for the focused window.
/// # Safety
/// The caller must provide valid screen rectangles and frame HWNDs on the owning UI thread.
pub unsafe fn pin_window(style: crate::pin::Style, owner: HWND) -> Result<()> {
    pin_rect(window_target()?, style, owner)
}

/// Saves the frame rect to an explicit path, then puts the frame away.
/// # Safety
/// The caller must provide valid screen rectangles and frame HWNDs on the owning UI thread.
pub unsafe fn save_frame_to(hwnd: HWND, path: &std::path::Path) -> Result<()> {
    let rect = visible_rect(hwnd)?;
    let _ = ShowWindow(hwnd, SW_HIDE);
    let result = grab(rect).and_then(|shot| {
        shot.save_png_to(path).map(|()| {
            logline(&format!("capture: frame -> {}", path.display()));
        })
    });
    if result.is_err() {
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    } else {
        crate::frame::hide(hwnd);
    }
    result
}

/// Saves the frame rect, then puts the frame away. The frame is hidden during
/// the grab so neither its border nor its shadow can enter the image.
/// # Safety
/// The caller must provide valid screen rectangles and frame HWNDs on the owning UI thread.
pub unsafe fn save_frame(hwnd: HWND, dir: &str) -> Result<()> {
    let rect = visible_rect(hwnd)?;
    let _ = ShowWindow(hwnd, SW_HIDE);
    let result = grab(rect).and_then(|shot| {
        shot.save_png(dir).map(|path| {
            logline(&format!("capture: frame -> {}", path.display()));
        })
    });
    if result.is_err() {
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    } else {
        crate::frame::hide(hwnd);
    }
    result
}

/// Copies the frame rect. Non-pinned frames are put away afterwards; a pinned
/// frame is only hidden for the grab and shown again, so the pin survives.
/// # Safety
/// The caller must provide valid screen rectangles and frame HWNDs on the owning UI thread.
pub unsafe fn copy_frame(hwnd: HWND, keep_open: bool) -> Result<()> {
    let rect = visible_rect(hwnd)?;
    let _ = ShowWindow(hwnd, SW_HIDE);
    let result = grab(rect).and_then(|shot| shot.into_clipboard(hwnd));
    if result.is_err() {
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    } else if keep_open {
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
        logline("capture: frame -> clipboard, pinned frame kept");
    } else {
        logline("capture: frame -> clipboard, frame hidden");
        crate::frame::hide(hwnd);
    }
    result
}

unsafe fn visible_rect(hwnd: HWND) -> Result<RECT> {
    let mut rect = RECT::default();
    GetWindowRect(hwnd, &mut rect)?;
    anyhow::ensure!(
        rect.right > rect.left && rect.bottom > rect.top,
        "frame has no area"
    );
    Ok(rect)
}
