use anyhow::Result;
use windows::Win32::Foundation::HWND;

pub use flameshot_handoff::set_path;

fn hide(frame_id: isize) {
    unsafe { crate::frame::hide(HWND(frame_id as *mut _)) };
}

fn restore(frame_id: isize) {
    unsafe { crate::frame::ensure_visible(HWND(frame_id as *mut _)) };
}

/// Hand the current frame to Flameshot without moving frame ownership out of Draw.
///
/// # Safety
/// The caller must provide a valid frame HWND on its owning UI thread.
pub unsafe fn launch(frame: HWND) -> Result<()> {
    if crate::frame::is_recording(frame) {
        crate::logline("flameshot ignored: recording (stop it first)");
        return Ok(());
    }
    let rect = crate::frame::screen_rect(frame);
    flameshot_handoff::launch(frame.0 as isize, rect, hide, restore)
}
