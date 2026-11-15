#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        if let Err(e) = run() {
            kit::win::fatal_error("keyboard-shell", &format!("Error:\n{e:#}"));
        }
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        kit::unimplemented("keyboard-shell")
    }
}

#[cfg(windows)]
fn run() -> anyhow::Result<()> {
    use kit::tray::{Event, Tray};
    use kit::win;

    const APP: &str = "keyboard-shell";
    let hosted = win::hosted();
    let no_tray = hosted && std::env::args().any(|arg| arg == "--no-tray");
    let _single = win::ensure_single_instance(APP)?;
    let (config_path, config) = keyboard_shell::config::load()?;
    let mut shell = keyboard_shell::shell::Shell::install(&config)?;

    if no_tray {
        use kit::ipc::HostCommand;
        let link = kit::ipc::PluginLink::new()?;
        link.ready(keyboard_shell::menu_model(false), Some("running".into()))?;
        kit::tray::run_headless(None, |event| {
            if let Event::Wakeup(4) = event {
                while let Some(command) = link.try_recv() {
                    match command {
                        HostCommand::Invoke { id, .. } if id == "suspend" => {
                            match shell.toggle_suspended() {
                                Ok(()) => {
                                    let _ = link
                                        .menu(&keyboard_shell::menu_model(shell.is_suspended()));
                                }
                                Err(e) => {
                                    let _ = link.notify(
                                        "Shell Keys",
                                        format!("Cannot change suspension:\n{e:#}"),
                                    );
                                }
                            }
                        }
                        HostCommand::Quit { .. } => kit::tray::quit(),
                        _ => {}
                    }
                }
            }
        });
        return Ok(());
    }

    let mut tray = Tray::new(
        APP,
        kit::icon::rgba(kit::icon::KEYBOARD_SHELL, kit::icon::State::Normal),
    )?;
    refresh_menu(&mut tray, hosted, shell.is_suspended());
    tray.run(None, |tray, event| {
        if let Event::Menu(label) = event {
            match label.as_str() {
                keyboard_shell::SUSPEND_LABEL => {
                    if let Err(e) = shell.toggle_suspended() {
                        win::message_box(APP, &format!("Cannot change suspension:\n{e:#}"));
                    }
                    refresh_menu(tray, hosted, shell.is_suspended());
                }
                "Start with Windows" => {
                    let now = !win::autostart_enabled(APP);
                    if let Err(e) = win::set_autostart(APP, now) {
                        win::message_box(APP, &format!("Autostart failed:\n{e:#}"));
                    }
                    refresh_menu(tray, hosted, shell.is_suspended());
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
fn refresh_menu(tray: &mut kit::tray::Tray, hosted: bool, suspended: bool) {
    use kit::tray::Entry;
    let mut entries = keyboard_shell::menu_model(suspended).render();
    if !hosted {
        entries.push(Entry::Toggle(
            "Start with Windows",
            kit::win::autostart_enabled("keyboard-shell"),
        ));
    }
    entries.push(Entry::Item("Edit Config"));
    if !hosted {
        entries.push(Entry::Item("Reload"));
        entries.push(Entry::Item("Exit"));
    }
    tray.set_menu(&entries);
    let _ = tray.set_icon(kit::icon::rgba(
        kit::icon::KEYBOARD_SHELL,
        if suspended {
            kit::icon::State::Suspended
        } else {
            kit::icon::State::Normal
        },
    ));
}
