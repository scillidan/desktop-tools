//! Frame → Flameshot bridge. Flameshot owns the full screenshot editor; this
//! crate only hands it the frame rectangle and gets out of the way.

use anyhow::{Context, Result};
use kit::layout::Rect;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use windows::Win32::Foundation::{CloseHandle, HWND};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, HMONITOR, MONITORINFO, MONITOR_DEFAULTTOPRIMARY,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::UI::WindowsAndMessaging::{IsWindow, SetCursorPos};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

static LOGGER: Mutex<Option<fn(&str)>> = Mutex::new(None);
static PATH: Mutex<String> = Mutex::new(String::new());
static RUNNING: AtomicBool = AtomicBool::new(false);
static ON_START: Mutex<Option<fn()>> = Mutex::new(None);
static ON_END: Mutex<Option<fn()>> = Mutex::new(None);

pub fn set_session_hooks(on_start: fn(), on_end: fn()) {
    *ON_START.lock().unwrap_or_else(|e| e.into_inner()) = Some(on_start);
    *ON_END.lock().unwrap_or_else(|e| e.into_inner()) = Some(on_end);
}

fn session_hook(end: bool) {
    let hook = if end { &ON_END } else { &ON_START };
    if let Some(hook) = *hook.lock().unwrap_or_else(|e| e.into_inner()) {
        hook();
    }
}

pub fn set_logger(logger: fn(&str)) {
    *LOGGER.lock().unwrap_or_else(|e| e.into_inner()) = Some(logger);
}

fn logline(message: &str) {
    if let Some(logger) = *LOGGER.lock().unwrap_or_else(|e| e.into_inner()) {
        logger(message);
    }
}

pub fn set_path(path: String) {
    *PATH.lock().unwrap_or_else(|e| e.into_inner()) = path;
}

/// True from launch until Flameshot exits or startup fails.
pub fn is_active() -> bool {
    RUNNING.load(Ordering::Acquire)
}

/// Hide the frame, run `flameshot gui --region …`, and leave the frame hidden:
/// the handoff is a one-shot "borrow the frame's window" command. Frame hide
/// and failure recovery are supplied by its owner, not implemented here.
pub fn launch(
    frame_id: isize,
    rect: Rect,
    hide_frame: fn(isize),
    restore_frame: fn(isize),
) -> Result<()> {
    let path = PATH.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if path.trim().is_empty() {
        // Experimental and off by default; Flameshot pins a preset region to
        // the screen origin when it lies within 7 px of it.
        anyhow::bail!(
            "Flameshot handoff is disabled (experimental: a region within 7 px \
             of the screen origin pins at 0,0); set [external] flameshot to the \
             executable to try it"
        );
    }
    if RUNNING.swap(true, Ordering::SeqCst) {
        anyhow::bail!("a Flameshot session is already open");
    }
    // RegisterHotKey consumes the chord even if dispatch ignores its action.
    // The owner must release Screen's global keys before Flameshot opens.
    session_hook(false);
    let frame = HWND(frame_id as *mut _);
    // Flameshot treats --region as relative to the monitor under the cursor.
    let (ox, oy) = monitor_origin(frame);
    // A top-left within 7 px of the origin collapses Flameshot's selection.
    let (x, y, w, h) = inset_from_origin(rect.l - ox, rect.t - oy, rect.w(), rect.h());
    let region = format!("{w}x{h}+{x}+{y}");
    let _ = unsafe { SetCursorPos((rect.l + rect.r) / 2, (rect.t + rect.b) / 2) };
    hide_frame(frame_id);
    logline(&format!("external: flameshot {region}"));
    std::thread::spawn(move || {
        // Pin widgets live in the Flameshot daemon. Reuse an existing daemon
        // instead of starting another one on every handoff.
        match process_running(&path) {
            Ok(true) => logline("external: flameshot daemon reused"),
            Ok(false) => match Command::new(&path).creation_flags(CREATE_NO_WINDOW).spawn() {
                Ok(_) => logline("external: flameshot daemon started"),
                Err(e) => logline(&format!("warn: flameshot daemon failed: {e:#}")),
            },
            Err(e) => logline(&format!(
                "warn: cannot check Flameshot daemon; not starting a duplicate: {e:#}"
            )),
        }
        std::thread::sleep(std::time::Duration::from_millis(400));
        let result = Command::new(&path)
            .args(["gui", "--region", &region])
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .with_context(|| format!("cannot start {path}"));
        let failed = match result {
            Ok(status) => {
                logline(&format!("external: flameshot exited {status}"));
                false
            }
            Err(e) => {
                logline(&format!("warn: flameshot failed: {e:#}"));
                true
            }
        };
        RUNNING.store(false, Ordering::SeqCst);
        if failed {
            let frame = HWND(frame_id as *mut _);
            if unsafe { IsWindow(Some(frame)).as_bool() } {
                restore_frame(frame_id);
            }
        }
        session_hook(true);
    });
    Ok(())
}

fn executable_name(path: &str) -> String {
    let name = Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("flameshot.exe");
    if name.contains('.') {
        name.to_owned()
    } else {
        format!("{name}.exe")
    }
}

fn process_running(path: &str) -> Result<bool> {
    let expected = executable_name(path);
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)? };
    let result = (|| {
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        unsafe { Process32FirstW(snapshot, &mut entry)? };
        loop {
            let len = entry
                .szExeFile
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(entry.szExeFile.len());
            if String::from_utf16_lossy(&entry.szExeFile[..len]).eq_ignore_ascii_case(&expected) {
                return Ok(true);
            }
            if unsafe { Process32NextW(snapshot, &mut entry) }.is_err() {
                return Ok(false);
            }
        }
    })();
    unsafe { CloseHandle(snapshot)? };
    result
}

fn monitor_origin(hwnd: HWND) -> (i32, i32) {
    let monitor: HMONITOR = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTOPRIMARY) };
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        (info.rcMonitor.left, info.rcMonitor.top)
    } else {
        (0, 0)
    }
}

/// Flameshot pins a preset selection to (0,0) when its x or y is under 7 px,
/// so nudge a region that touches the screen origin inward by that margin and
/// shrink it to match.
fn inset_from_origin(x: i32, y: i32, w: i32, h: i32) -> (i32, i32, i32, i32) {
    let mut x = x;
    let mut y = y;
    let mut w = w;
    let mut h = h;
    if x < 7 {
        w -= 7 - x;
        x = 7;
    }
    if y < 7 {
        h -= 7 - y;
        y = 7;
    }
    (x, y, w.max(1), h.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn executable_name_is_normalized_for_process_lookup() {
        assert_eq!(executable_name("flameshot"), "flameshot.exe");
        assert_eq!(executable_name(r"C:\Tools\Flameshot.exe"), "Flameshot.exe");
    }

    #[test]
    fn edge_touching_region_is_inset() {
        assert_eq!(inset_from_origin(960, 0, 960, 516), (960, 7, 960, 509));
        assert_eq!(inset_from_origin(0, 216, 300, 200), (7, 216, 293, 200));
        assert_eq!(inset_from_origin(560, 216, 300, 200), (560, 216, 300, 200));
    }
}
