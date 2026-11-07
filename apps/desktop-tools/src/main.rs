#![cfg_attr(windows, windows_subsystem = "windows")]

//! desktop-tools suite host. It owns the profile, core.toml, one tray and
//! the lifecycle of the five plugin processes. Plugin code never runs in this
//! process; P7 keeps each plugin's local tray.

#[cfg(windows)]
mod host;
#[cfg(windows)]
mod job;

fn main() -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        if let Err(e) = host::run() {
            kit::win::message_box("desktop-tools", &format!("Error:\n{e:#}"));
            return Err(e);
        }
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        kit::unimplemented("desktop-tools")
    }
}
