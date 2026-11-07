#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        if let Err(e) = run() {
            kit::win::fatal_error("keyboard-keymap", &format!("Error:\n{e:#}"));
        }
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        kit::unimplemented("keyboard-keymap")
    }
}

#[cfg(windows)]
fn run() -> anyhow::Result<()> {
    use kit::tray::{Event, Tray};
    use kit::win;

    const APP: &str = "keyboard-keymap";
    let hosted = win::hosted();
    let no_tray = hosted && std::env::args().any(|arg| arg == "--no-tray");
    let _single = win::ensure_single_instance(APP)?;
    let (config_path, engine) = keyboard_keymap::config::load()?;
    let mut hook = keyboard_keymap::hook::HookGuard::install(engine)?;
    let mut last_modified = modified(&config_path);
    let _timer = ReloadTimer::new()?;

    if no_tray {
        use kit::ipc::HostCommand;
        let link = kit::ipc::PluginLink::new()?;
        link.ready(
            kit::menu::MenuModel::new(Vec::new()),
            Some("running".into()),
        )?;
        kit::tray::run_headless(None, |event| match event {
            Event::Wakeup(4) => {
                while let Some(command) = link.try_recv() {
                    match command {
                        HostCommand::Invoke { id, .. } if id == "edit" => {
                            win::edit_config(&config_path)
                        }
                        HostCommand::Reload { .. } => {
                            if let Err(e) = reload_config(&config_path, &mut hook) {
                                let _ = link.notify(
                                    "Key Map — reload failed, old config kept",
                                    format!("{e:#}"),
                                );
                            }
                            last_modified = modified(&config_path);
                        }
                        HostCommand::Quit { .. } => kit::tray::quit(),
                        _ => {}
                    }
                }
            }
            Event::Timer(1) => {
                if reload_changed(&config_path, &mut last_modified) {
                    if let Err(e) = reload_config(&config_path, &mut hook) {
                        let _ = link
                            .notify("Key Map — reload failed, old config kept", format!("{e:#}"));
                    }
                }
            }
            Event::Wakeup(keyboard_keymap::hook::WM_KEYMAP_ERROR) => {
                if let Some(error) = keyboard_keymap::hook::take_error() {
                    let _ = link.notify("Key Map", error);
                }
            }
            _ => {}
        });
        return Ok(());
    }

    let mut tray = Tray::new(
        APP,
        kit::icon::letter_rgba('k', [0x6a, 0x51, 0xa3, 0xff], [0x42, 0x32, 0x66, 0xff]),
    )?;
    refresh_menu(&mut tray, hosted);
    tray.run(None, |tray, event| match event {
        Event::Menu(label) => match label.as_str() {
            "Start with Windows" => {
                let now = !win::autostart_enabled(APP);
                if let Err(e) = win::set_autostart(APP, now) {
                    win::message_box(APP, &format!("Autostart failed:\n{e:#}"));
                }
                refresh_menu(tray, hosted);
            }
            "Edit Config" => win::edit_config(&config_path),
            "Reload" => {
                if let Err(e) = reload_config(&config_path, &mut hook) {
                    let _ = tray.notify(
                        "Key Map — reload failed, old config kept",
                        &format!("{e:#}"),
                    );
                }
                last_modified = modified(&config_path);
            }
            "Exit" => tray.quit(),
            _ => {}
        },
        Event::Timer(1) => {
            if reload_changed(&config_path, &mut last_modified) {
                if let Err(e) = reload_config(&config_path, &mut hook) {
                    let _ = tray.notify(
                        "Key Map — reload failed, old config kept",
                        &format!("{e:#}"),
                    );
                }
            }
        }
        Event::Wakeup(keyboard_keymap::hook::WM_KEYMAP_ERROR) => {
            if let Some(error) = keyboard_keymap::hook::take_error() {
                let _ = tray.notify("Key Map", &error);
            }
        }
        _ => {}
    });
    Ok(())
}

#[cfg(windows)]
fn modified(path: &std::path::Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

#[cfg(windows)]
fn reload_changed(path: &std::path::Path, previous: &mut Option<std::time::SystemTime>) -> bool {
    let current = modified(path);
    let changed = current != *previous;
    *previous = current;
    changed && path.exists()
}

#[cfg(windows)]
fn reload_config(
    path: &std::path::Path,
    hook: &mut keyboard_keymap::hook::HookGuard,
) -> anyhow::Result<()> {
    let root = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("config has no parent directory"))?;
    let (_, engine) = keyboard_keymap::config::load_from(root.to_path_buf())?;
    hook.replace(engine)
}

#[cfg(windows)]
struct ReloadTimer(usize);

#[cfg(windows)]
impl ReloadTimer {
    fn new() -> anyhow::Result<Self> {
        use windows::Win32::UI::WindowsAndMessaging::SetTimer;
        let id = unsafe { SetTimer(None, 1, 500, None) };
        if id == 0 {
            anyhow::bail!("SetTimer failed");
        }
        Ok(Self(id))
    }
}

#[cfg(windows)]
impl Drop for ReloadTimer {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::KillTimer(None, self.0);
        }
    }
}

#[cfg(windows)]
fn refresh_menu(tray: &mut kit::tray::Tray, hosted: bool) {
    use kit::tray::Entry;
    let mut entries = Vec::new();
    if !hosted {
        entries.push(Entry::Toggle(
            "Start with Windows",
            kit::win::autostart_enabled("keyboard-keymap"),
        ));
    }
    entries.push(Entry::Item("Edit Config"));
    if !hosted {
        entries.push(Entry::Item("Reload"));
        entries.push(Entry::Item("Exit"));
    }
    tray.set_menu(&entries);
}
