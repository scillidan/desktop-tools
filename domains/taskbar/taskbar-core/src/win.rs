//! Small Win32 helpers used by the taskbar workers.

use anyhow::Result;

pub fn hide_owned_console() {
    use windows::Win32::System::Console::{GetConsoleProcessList, GetConsoleWindow};
    use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};
    unsafe {
        let console = GetConsoleWindow();
        let mut processes = [0u32; 1];
        if !console.0.is_null() && GetConsoleProcessList(&mut processes) == 1 {
            let _ = ShowWindow(console, SW_HIDE);
        }
    }
}

pub fn attach_parent_console() {
    use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
    unsafe {
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

pub fn post_thread_message(thread: u32, wparam: usize, lparam: isize) -> Result<()> {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_APP};
    unsafe { PostThreadMessageW(thread, WM_APP, WPARAM(wparam), LPARAM(lparam))? };
    Ok(())
}

/// Hosted stderr is captured in the plugin log; standalone diagnostics retain
/// the same stream instead of creating module-specific files beside the exe.
pub fn log(app: &str, text: &str) {
    eprintln!("{app}: {text}");
}

pub fn foreground_hwnd() -> Option<windows::Win32::Foundation::HWND> {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    let hwnd = unsafe { GetForegroundWindow() };
    (!hwnd.0.is_null()).then_some(hwnd)
}
