#![cfg_attr(windows, windows_subsystem = "windows")]

mod center_taskbar;
#[cfg(any(windows, test))]
mod config;
mod hidden_taskbar;
mod taskbar_pin;

const APP: &str = "taskbar-tools";
const SUSPEND: &str = "Suspend Group \"Taskbar\"";

#[cfg(windows)]
static SHUTDOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[cfg(windows)]
static PINS_PAUSED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[cfg(windows)]
static HOTKEYS_SUSPENDED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[cfg(windows)]
static NOTIFIER: std::sync::OnceLock<kit::ipc::PluginNotifier> = std::sync::OnceLock::new();

#[cfg(windows)]
pub fn shutting_down() -> bool {
    SHUTDOWN.load(std::sync::atomic::Ordering::Relaxed)
}

#[cfg(windows)]
pub fn pins_paused() -> bool {
    PINS_PAUSED.load(std::sync::atomic::Ordering::Relaxed)
}

#[cfg(windows)]
pub fn hotkeys_suspended() -> bool {
    HOTKEYS_SUSPENDED.load(std::sync::atomic::Ordering::Relaxed)
}

#[cfg(windows)]
pub(crate) fn report(title: &str, text: &str) {
    eprintln!("{title}: {text}");
    if let Some(notifier) = NOTIFIER.get() {
        let _ = notifier.notify(title, text);
    } else {
        kit::win::message_box(title, text);
    }
}

#[cfg(windows)]
pub fn wait_ms(mut milliseconds: u64) {
    while milliseconds > 0 && !shutting_down() {
        let step = milliseconds.min(50);
        std::thread::sleep(std::time::Duration::from_millis(step));
        milliseconds -= step;
    }
}

fn main() -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        if let Err(error) = run() {
            kit::win::fatal_error(APP, &format!("Error:\n{error:#}"));
        }
        Ok(())
    }
    #[cfg(target_os = "linux")]
    kit::unimplemented(APP)
}

#[cfg(windows)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    Daemon,
    Dump,
    Inspect,
    Once,
}

#[cfg(windows)]
fn action() -> anyhow::Result<Action> {
    let mut action = Action::Daemon;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--plugin" | "--no-tray" => {}
            "--profile" | "--wait-pid" => {
                anyhow::ensure!(args.next().is_some(), "{arg} requires a value");
            }
            "--dump" if action == Action::Daemon => action = Action::Dump,
            "--inspect" if action == Action::Daemon => action = Action::Inspect,
            "--once" if action == Action::Daemon => action = Action::Once,
            "--help" | "-h" => {
                println!("usage: taskbar-tools [--profile DIR] [--dump|--inspect|--once]");
                std::process::exit(0);
            }
            _ => anyhow::bail!("usage: taskbar-tools [--profile DIR] [--dump|--inspect|--once]"),
        }
    }
    Ok(action)
}

#[cfg(windows)]
fn run() -> anyhow::Result<()> {
    let action = action()?;
    if action != Action::Daemon {
        taskbar_core::win::attach_parent_console();
        let automation = taskbar_core::shell::Automation::new()?;
        return match action {
            Action::Dump => taskbar_pin::dump(&automation),
            Action::Inspect => {
                taskbar_pin::inspect(&automation)?;
                drop(automation);
                center_taskbar::inspect()
            }
            Action::Once => taskbar_pin::once(&automation),
            Action::Daemon => unreachable!(),
        };
    }
    run_daemon()
}

#[cfg(windows)]
fn run_daemon() -> anyhow::Result<()> {
    use windows::Win32::UI::HiDpi::{
        SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    };

    let _instance = kit::win::ensure_single_instance(APP)?;
    taskbar_core::win::hide_owned_console();
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)? };
    let _ = config::load()?;
    let config_path = config::path()?;
    let hosted = kit::win::hosted() && std::env::args().any(|arg| arg == "--no-tray");
    if hosted {
        run_hosted()
    } else {
        run_standalone(config_path)
    }
}

#[cfg(windows)]
fn workers() -> anyhow::Result<Vec<std::thread::JoinHandle<()>>> {
    [
        spawn("taskbar-pin", taskbar_pin::run),
        spawn("center-taskbar", center_taskbar::run),
        spawn("hidden-taskbar", hidden_taskbar::run),
    ]
    .into_iter()
    .collect()
}

#[cfg(windows)]
fn run_hosted() -> anyhow::Result<()> {
    use kit::ipc::{HostCommand, PluginLink};
    use kit::tray::Event;

    let link = PluginLink::new()?;
    let _ = NOTIFIER.set(link.notifier());
    link.ready(menu_model(true, false), Some("running".into()))?;
    let handles = workers()?;
    kit::tray::run_headless(None, |event| {
        if let Event::Wakeup(4) = event {
            while let Some(command) = link.try_recv() {
                match command {
                    HostCommand::Invoke { id, .. } if id == "unpin" => {
                        PINS_PAUSED.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                    HostCommand::Invoke { id, .. } if id == "suspend" => {
                        let value = !hotkeys_suspended();
                        HOTKEYS_SUSPENDED.store(value, std::sync::atomic::Ordering::Relaxed);
                        let _ = link.menu(&menu_model(true, value));
                    }
                    HostCommand::Reload { .. } => {
                        PINS_PAUSED.store(false, std::sync::atomic::Ordering::Relaxed);
                    }
                    HostCommand::Quit { .. } => shutdown(),
                    _ => {}
                }
            }
        }
    });
    join_workers(handles);
    Ok(())
}

#[cfg(windows)]
fn run_standalone(config_path: std::path::PathBuf) -> anyhow::Result<()> {
    use kit::tray::{Event, Tray};

    let mut tray = Tray::new(
        APP,
        kit::icon::letter_rgba('t', [0x50, 0xa0, 0xff, 0xff], [0x20, 0x40, 0x70, 0xff]),
    )?;
    tray.set_menu(&menu_model(false, false).render());
    let handles = workers()?;
    tray.run(None, |tray, event| {
        if let Event::Menu(id) = event {
            match id.as_str() {
                "unpin" => PINS_PAUSED.store(true, std::sync::atomic::Ordering::Relaxed),
                "suspend" => {
                    let value = !hotkeys_suspended();
                    HOTKEYS_SUSPENDED.store(value, std::sync::atomic::Ordering::Relaxed);
                    tray.set_menu(&menu_model(false, value).render());
                }
                "autostart" => {
                    let value = !kit::win::autostart_enabled(APP);
                    if let Err(e) = kit::win::set_autostart(APP, value) {
                        report(APP, &format!("Autostart failed:\n{e:#}"));
                    }
                    tray.set_menu(&menu_model(false, hotkeys_suspended()).render());
                }
                "edit" => kit::win::edit_config(&config_path),
                "quit" => {
                    shutdown();
                    tray.quit();
                }
                _ => {}
            }
        }
    });
    join_workers(handles);
    Ok(())
}

#[cfg(windows)]
fn shutdown() {
    SHUTDOWN.store(true, std::sync::atomic::Ordering::Relaxed);
    hidden_taskbar::wake_for_shutdown();
    kit::tray::quit();
}

#[cfg(windows)]
fn join_workers(handles: Vec<std::thread::JoinHandle<()>>) {
    for handle in handles {
        let _ = handle.join();
    }
}

#[cfg(windows)]
fn spawn(
    name: &'static str,
    run: fn() -> anyhow::Result<()>,
) -> anyhow::Result<std::thread::JoinHandle<()>> {
    std::thread::Builder::new()
        .name(name.to_string())
        .spawn(move || {
            while !shutting_down() {
                match std::panic::catch_unwind(run) {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => taskbar_core::win::log(name, &format!("{error:#}")),
                    Err(_) => taskbar_core::win::log(name, "worker panicked"),
                }
                wait_ms(1000);
            }
        })
        .map_err(Into::into)
}

fn menu_model(hosted: bool, suspended: bool) -> kit::menu::MenuModel {
    use kit::menu::MenuItem;
    let mut items = vec![
        MenuItem::Item {
            id: "unpin".into(),
            label: "Unpin all".into(),
            enabled: true,
        },
        MenuItem::Toggle {
            id: "suspend".into(),
            label: SUSPEND.into(),
            checked: suspended,
            enabled: true,
        },
    ];
    if !hosted {
        items.push(MenuItem::Separator);
        items.push(MenuItem::Toggle {
            id: "autostart".into(),
            label: "Start with Windows".into(),
            checked: kit::win::autostart_enabled(APP),
            enabled: true,
        });
        items.push(MenuItem::Item {
            id: "edit".into(),
            label: "Edit Config".into(),
            enabled: true,
        });
        items.push(MenuItem::Item {
            id: "quit".into(),
            label: "Exit".into(),
            enabled: true,
        });
    }
    kit::menu::MenuModel::new(items)
}

#[cfg(test)]
mod tests {
    #[test]
    fn hosted_and_standalone_menu_models_are_valid() {
        super::menu_model(true, false).validate().unwrap();
        super::menu_model(false, true).validate().unwrap();
    }
}
