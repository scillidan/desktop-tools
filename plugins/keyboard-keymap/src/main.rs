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
    let (config_path, config) = keyboard_keymap::config::load()?;
    let _hook = keyboard_keymap::shell::install(&config)?;

    if no_tray {
        use kit::ipc::HostCommand;
        let link = kit::ipc::PluginLink::new()?;
        link.ready(kit::menu::MenuModel::default(), Some("running".into()))?;
        kit::tray::run_headless(None, |event| {
            if let Event::Wakeup(4) = event {
                while let Some(command) = link.try_recv() {
                    if matches!(command, HostCommand::Quit { .. }) {
                        kit::tray::quit();
                    }
                }
            }
        });
        return Ok(());
    }

    let mut tray = Tray::new(
        APP,
        kit::icon::letter_rgba('k', [0x6a, 0x51, 0xa3, 0xff], [0x42, 0x32, 0x66, 0xff]),
    )?;
    refresh_menu(&mut tray, hosted);
    tray.run(None, |tray, event| {
        if let Event::Menu(label) = event {
            match label.as_str() {
                "Start with Windows" => {
                    let now = !win::autostart_enabled(APP);
                    if let Err(e) = win::set_autostart(APP, now) {
                        win::message_box(APP, &format!("Autostart failed:\n{e:#}"));
                    }
                    refresh_menu(tray, hosted);
                }
                "Edit Config" => win::edit_config(&config_path),
                "Reload" => {
                    kit::config::restart_self();
                    tray.quit();
                }
                "Exit" => tray.quit(),
                _ => {}
            }
        }
    });
    Ok(())
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
