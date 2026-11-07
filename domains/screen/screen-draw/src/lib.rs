//! Screen selection, frame, OCR overlay, pin and virtual-desktop coordination.

pub mod capture;
pub mod desktops;
pub mod flameshot;
pub mod frame;
pub mod interaction;
pub mod mask;
pub mod overlay;
pub mod pin;

use std::os::windows::process::CommandExt;
use std::sync::OnceLock;
use std::time::SystemTime;
use windows::Win32::Foundation::HWND;

static LOGGER: OnceLock<fn(&str)> = OnceLock::new();
static NOTIFIER: OnceLock<fn(&str, &str)> = OnceLock::new();

pub fn set_logger(logger: fn(&str)) {
    let _ = LOGGER.set(logger);
}

pub fn set_notifier(notifier: fn(&str, &str)) {
    let _ = NOTIFIER.set(notifier);
}

pub(crate) fn logline(message: &str) {
    if let Some(logger) = LOGGER.get() {
        logger(message);
    }
}

pub(crate) fn notify(title: &str, body: &str) {
    if let Some(notifier) = NOTIFIER.get() {
        notifier(title, body);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PinBorder {
    Hatch,
    Line,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaskFill {
    Boxes,
    Lines,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaskMode {
    Box,
    BoxMask,
    BoxOutside,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaskSize {
    /// Legacy: size each line from its OCR box height.
    Auto,
    /// One fixed size for all text, in points (WezTerm's `font_size` unit).
    Pt(u32),
}

/// Preferences needed to create a frame on a virtual desktop.
pub struct FrameSettings {
    pub frame_w: i32,
    pub frame_h: i32,
    pub work_area: bool,
    pub show_hint: bool,
    pub font: Option<String>,
    pub ocr_scale: u32,
    pub ocr_language: Option<String>,
}

static FRAME_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

pub fn frame_hwnd() -> HWND {
    HWND(FRAME_HWND.load(std::sync::atomic::Ordering::SeqCst) as *mut core::ffi::c_void)
}

fn set_frame_hwnd(hwnd: HWND) {
    FRAME_HWND.store(hwnd.0 as isize, std::sync::atomic::Ordering::SeqCst);
}

pub fn ocr_frame_ops() -> screen_ocr::FrameOps {
    screen_ocr::FrameOps {
        note_start: frame::note_ocr_start,
        screen_rect: frame::screen_rect,
        scale: frame::ocr_scale,
        set_running: frame::set_ocr_running,
        language: frame::ocr_language,
    }
}

static CAPTURE_DIR: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

fn capture_dir() -> String {
    CAPTURE_DIR
        .lock()
        .map(|d| d.clone())
        .unwrap_or_else(|e| e.into_inner().clone())
}

pub fn set_capture_dir(dir: &str) {
    if let Ok(mut d) = CAPTURE_DIR.lock() {
        *d = dir.to_string();
    }
}

static PIN_STYLE: std::sync::Mutex<pin::Style> = std::sync::Mutex::new(pin::Style::DEFAULT);

pub fn set_pin_style(border: PinBorder, line_color: u32, hatch_color: u32) {
    if let Ok(mut style) = PIN_STYLE.lock() {
        *style = pin::Style {
            border,
            line_color,
            hatch_color,
        };
    }
}

pub fn pin_style() -> pin::Style {
    PIN_STYLE
        .lock()
        .map(|style| *style)
        .unwrap_or(pin::Style::DEFAULT)
}

static EXTERNAL_EDITOR: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

pub fn set_external_editor(editor: &str) {
    if let Ok(mut e) = EXTERNAL_EDITOR.lock() {
        *e = editor.to_string();
    }
}

fn external_editor() -> String {
    EXTERNAL_EDITOR
        .lock()
        .map(|e| e.clone())
        .unwrap_or_else(|e| e.into_inner().clone())
}

/// Focused-frame Ctrl+Shift+S: straight to the capture directory.
pub fn frame_quick_save(frame: HWND) {
    if let Err(e) = unsafe { capture::save_frame(frame, &capture_dir()) } {
        logline(&format!("save frame failed: {e:#}"));
    }
}

/// Focused-frame Ctrl+S: choose a destination, then capture after the dialog fades.
pub fn frame_save_as(frame: HWND) {
    let name = capture::default_capture_name();
    let path = match unsafe { capture::choose_file(frame, &capture_dir(), &name) } {
        Ok(Some(path)) => path,
        Ok(None) => return,
        Err(e) => {
            logline(&format!("save dialog failed: {e:#}"));
            return;
        }
    };
    std::thread::sleep(std::time::Duration::from_millis(220));
    if let Err(e) = unsafe { capture::save_frame_to(frame, &path) } {
        logline(&format!("save frame failed: {e:#}"));
    }
}

pub fn frame_copy(frame: HWND) {
    let keep_open = unsafe { frame::is_pinned(frame) };
    if let Err(e) = unsafe { capture::copy_frame(frame, keep_open) } {
        logline(&format!("copy frame failed: {e:#}"));
    }
}

pub fn frame_pin(frame: HWND) {
    let result = unsafe {
        match frame::visible_rect(frame) {
            Some(rect) => {
                frame::hide(frame);
                let result = capture::pin_rect(rect, pin_style());
                if result.is_err() {
                    frame::ensure_visible(frame);
                }
                result
            }
            None => return,
        }
    };
    if let Err(e) = result {
        logline(&format!("pin frame failed: {e:#}"));
    }
}

/// Temp files handed to an external editor live here; a day of files is kept.
const TEMP_KEEP_SECS: u64 = 24 * 60 * 60;

fn temp_capture_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("screen-tools-rs");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Delete stale handoff PNGs from both current and legacy locations.
pub fn cleanup_temp_captures() {
    let mut roots = vec![std::env::temp_dir()];
    roots.push(temp_capture_dir());
    roots.push(std::env::temp_dir().join("dt-ocr"));
    let now = SystemTime::now();
    let mut removed = 0;
    for root in roots {
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let is_capture = path.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                (n.starts_with("screen-tools-rs-") || n.starts_with("dt-ocr-"))
                    && n.ends_with(".png")
            });
            if !is_capture {
                continue;
            }
            let stale = entry
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| now.duration_since(t).ok())
                .is_some_and(|age| age.as_secs() > TEMP_KEEP_SECS);
            if stale && std::fs::remove_file(&path).is_ok() {
                removed += 1;
            }
        }
    }
    if removed > 0 {
        logline(&format!("temp: removed {removed} stale capture(s)"));
    }
}

/// Focused-frame Ctrl+O: capture to a temporary PNG and open the configured editor.
pub fn frame_open_with(frame: HWND) {
    let editor = external_editor();
    if editor.trim().is_empty() {
        logline("open with: [external] editor is empty");
        return;
    }
    cleanup_temp_captures();
    let path = temp_capture_dir().join(capture::default_capture_name());
    if let Err(e) = unsafe { capture::save_frame_to(frame, &path) } {
        logline(&format!("open with: save failed: {e:#}"));
        return;
    }
    match std::process::Command::new(&editor)
        .arg(&path)
        .creation_flags(0x0800_0000)
        .spawn()
    {
        Ok(_) => logline(&format!("open with: {editor} <- {}", path.display())),
        Err(e) => logline(&format!("open with failed: {e:#}")),
    }
}
