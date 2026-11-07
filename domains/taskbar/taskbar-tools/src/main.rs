#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(any(windows, test))]
mod config;

const APP: &str = "taskbar-tools";
const SUSPEND: &str = "Suspend Group \"Taskbar\"";

#[cfg(windows)]
static SHUTDOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[cfg(windows)]
static PINS_PAUSED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[cfg(windows)]
static SUSPENDED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
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
pub fn suspended() -> bool {
    SUSPENDED.load(std::sync::atomic::Ordering::Relaxed)
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
                taskbar_center::inspect()
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
    config::load()?;
    let hosted = kit::win::hosted() && std::env::args().any(|arg| arg == "--no-tray");
    if hosted {
        run_hosted()
    } else {
        run_standalone()
    }
}

#[cfg(windows)]
fn workers() -> anyhow::Result<Vec<std::thread::JoinHandle<()>>> {
    [
        spawn("center-taskbar", || {
            taskbar_center::run(shutting_down, suspended)
        }),
        spawn("hidden-taskbar", || {
            taskbar_hidden::run(shutting_down, suspended, report)
        }),
        spawn("taskbar-pin", || {
            taskbar_pin::run(shutting_down, || pins_paused() || suspended())
        }),
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
                    HostCommand::Invoke { id, .. } => {
                        let result = match id.as_str() {
                            "unpin" => {
                                PINS_PAUSED.store(true, std::sync::atomic::Ordering::Relaxed);
                                Ok(())
                            }
                            "suspend" => {
                                let value = !suspended();
                                SUSPENDED.store(value, std::sync::atomic::Ordering::Relaxed);
                                let _ = link.menu(&menu_model(true, value));
                                Ok(())
                            }
                            "edit-center" => config::edit(config::Module::Center),
                            "edit-hidden" => config::edit(config::Module::Hidden),
                            "edit-pin" => config::edit(config::Module::Pin),
                            _ => Ok(()),
                        };
                        if let Err(error) = result {
                            report(APP, &format!("Edit config failed:\n{error:#}"));
                        }
                    }
                    HostCommand::Reload { .. } => {
                        PINS_PAUSED.store(false, std::sync::atomic::Ordering::Relaxed);
                    }
                    HostCommand::Quit { .. } => shutdown(),
                }
            }
        }
    });
    join_workers(handles);
    taskbar_core::win::log(APP, "shutdown complete");
    Ok(())
}

#[cfg(windows)]
fn run_standalone() -> anyhow::Result<()> {
    use kit::tray::Tray;

    let mut tray = Tray::new(
        APP,
        kit::icon::letter_rgba('t', [0x50, 0xa0, 0xff, 0xff], [0x20, 0x40, 0x70, 0xff]),
    )?;
    tray.set_menu(&menu_model(false, false).render());
    let handles = workers()?;
    tray.run(None, |tray, event| {
        if let Some(id) = standalone_event_id(event) {
            match id.as_str() {
                "unpin" => PINS_PAUSED.store(true, std::sync::atomic::Ordering::Relaxed),
                "suspend" => {
                    let value = !suspended();
                    SUSPENDED.store(value, std::sync::atomic::Ordering::Relaxed);
                    tray.set_menu(&menu_model(false, value).render());
                }
                "autostart" => {
                    let value = !kit::win::autostart_enabled(APP);
                    if let Err(e) = kit::win::set_autostart(APP, value) {
                        report(APP, &format!("Autostart failed:\n{e:#}"));
                    }
                    tray.set_menu(&menu_model(false, suspended()).render());
                }
                "edit-center" | "edit-hidden" | "edit-pin" => {
                    let module = match id.as_str() {
                        "edit-center" => config::Module::Center,
                        "edit-hidden" => config::Module::Hidden,
                        _ => config::Module::Pin,
                    };
                    if let Err(error) = config::edit(module) {
                        report(APP, &format!("Edit config failed:\n{error:#}"));
                    }
                }
                "quit" => {
                    shutdown();
                    tray.quit();
                }
                _ => {}
            }
        }
    });
    join_workers(handles);
    taskbar_core::win::log(APP, "shutdown complete");
    Ok(())
}

#[cfg(windows)]
fn standalone_event_id(event: kit::tray::Event) -> Option<String> {
    match event {
        kit::tray::Event::Menu(id) | kit::tray::Event::MenuId(id) => Some(id),
        _ => None,
    }
}

#[cfg(windows)]
fn shutdown() {
    taskbar_core::win::log(APP, "shutdown requested");
    SHUTDOWN.store(true, std::sync::atomic::Ordering::Relaxed);
    taskbar_hidden::wake_for_shutdown();
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
    run: impl Fn() -> anyhow::Result<()> + Send + 'static + std::panic::RefUnwindSafe,
) -> anyhow::Result<std::thread::JoinHandle<()>> {
    std::thread::Builder::new()
        .name(name.to_string())
        .spawn(move || {
            while !shutting_down() {
                match std::panic::catch_unwind(&run) {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => taskbar_core::win::log(name, &format!("{error:#}")),
                    Err(_) => taskbar_core::win::log(name, "worker panicked"),
                }
                taskbar_core::win::wait_ms(1000, shutting_down);
            }
        })
        .map_err(Into::into)
}

fn menu_model(hosted: bool, suspended: bool) -> kit::menu::MenuModel {
    use kit::menu::MenuItem;

    fn feature(label: &str, description: &str, mut actions: Vec<MenuItem>) -> MenuItem {
        let mut items = vec![
            MenuItem::Info {
                label: description.into(),
            },
            MenuItem::Separator,
        ];
        items.append(&mut actions);
        MenuItem::Submenu {
            label: label.into(),
            items,
        }
    }

    let edit = |id: &str, label: &str| MenuItem::Item {
        id: id.into(),
        label: label.into(),
        enabled: true,
    };
    let mut items = vec![
        feature(
            "Center Taskbar",
            "Center taskbar buttons without changing their order.",
            vec![edit("edit-center", "Edit Center Taskbar config")],
        ),
        feature(
            "Hidden Taskbar",
            "Hide or restore the foreground monitor taskbar.",
            vec![edit("edit-hidden", "Edit Hidden Taskbar config")],
        ),
        feature(
            "Taskbar Pin",
            "Keep configured applications ahead of other taskbar groups.",
            vec![
                MenuItem::Item {
                    id: "unpin".into(),
                    label: "Unpin all".into(),
                    enabled: true,
                },
                edit("edit-pin", "Edit Taskbar Pin config"),
            ],
        ),
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
            id: "quit".into(),
            label: "Exit".into(),
            enabled: true,
        });
    }
    kit::menu::MenuModel::new(items)
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    #[test]
    fn standalone_routes_stable_menu_ids() {
        let event = kit::tray::Event::MenuId("quit".into());
        assert_eq!(super::standalone_event_id(event).as_deref(), Some("quit"));
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "manipulates the real Explorer taskbar"]
    fn workers_stop_cleanly() {
        let handles = super::workers().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(500));
        super::shutdown();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            super::join_workers(handles);
            let _ = done_tx.send(());
        });
        done_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("workers must finish within five seconds of shutdown");
    }

    #[test]
    fn hosted_and_standalone_menu_models_are_valid() {
        let hosted = super::menu_model(true, false);
        hosted.validate().unwrap();
        super::menu_model(false, true).validate().unwrap();
        assert_eq!(hosted.id_for_label("Unpin all"), Some("unpin"));
        assert_eq!(hosted.id_for_label("Edit Config"), None);
        assert_eq!(
            hosted.id_for_label("Edit Center Taskbar config"),
            Some("edit-center")
        );
        for item in &hosted.items[..3] {
            let kit::menu::MenuItem::Submenu { items, .. } = item else {
                panic!("feature must be a submenu")
            };
            assert!(matches!(items[0], kit::menu::MenuItem::Info { .. }));
            assert!(matches!(items[1], kit::menu::MenuItem::Separator));
        }
    }
}
