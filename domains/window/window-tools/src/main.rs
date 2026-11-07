//! window-tools — window management for the desktop-tools suite.
//!
//!   window-tools                    run as a daemon (window-tools.toml)
//!   window-tools --action NAME      apply one action to the foreground window
//!   window-tools --pid PID          target a process (with --action)
//!   window-tools --config PATH      use a different config file
//!   window-tools --profile DIR      use a profile directory
//!   window-tools --version

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod config;
pub use window_core::keys;
pub use window_rectangle::geometry as layout;
mod menu;
mod platform;

#[cfg(windows)]
mod history;
mod kbdhook;
#[cfg(windows)]
mod screen;
#[cfg(windows)]
mod tray;
#[cfg(windows)]
mod win32;
#[cfg(target_os = "linux")]
mod x11;

use anyhow::{Context, Result};
use config::Action;
use platform::Platform;
use std::path::PathBuf;
use std::str::FromStr;

struct Args {
    action: Option<String>,
    pid: Option<u32>,
    config: Option<PathBuf>,
    version: bool,
    plugin: bool,
    _no_tray: bool,
}

fn parse_args() -> Result<Args> {
    let mut args = Args {
        action: None,
        pid: None,
        config: None,
        version: false,
        plugin: false,
        _no_tray: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        let mut split = a.splitn(2, '=');
        let (flag, inline_val) = (split.next().unwrap_or(""), split.next());
        let value = |it: &mut dyn Iterator<Item = String>| -> Result<String> {
            inline_val
                .map(str::to_string)
                .or_else(|| it.next())
                .with_context(|| format!("flag {flag} needs a value"))
        };
        match flag {
            "--action" | "-a" => args.action = Some(value(&mut it)?),
            "--pid" | "-p" => {
                args.pid = Some(value(&mut it)?.parse().context("--pid needs a number")?)
            }
            "--config" | "-c" => args.config = Some(PathBuf::from(value(&mut it)?)),
            "--profile" => {
                let _ = value(&mut it)?;
            }
            "--plugin" => args.plugin = true,
            "--no-tray" => args._no_tray = true,
            "--version" | "-V" => args.version = true,
            "--help" | "-h" => {
                println!("window-tools [--action NAME [--pid PID]] [--config PATH] [--profile DIR] [--plugin --no-tray]");
                std::process::exit(0);
            }
            other => anyhow::bail!("unknown flag `{other}` (try --help)"),
        }
    }
    Ok(args)
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .try_init();
    let args = parse_args()?;
    if args.version {
        println!("window-tools {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    // One-shot mode: `--action moveToLeft [--pid 1234]` then exit.
    if let Some(name) = args.action {
        let action = Action::from_str(&name)?;
        if action == Action::Undo {
            anyhow::bail!("`undo` only works in daemon mode (it restores the previous snap)");
        }
        let presets = if action == Action::SizerMenu {
            let cfg_path = match &args.config {
                Some(p) => p.clone(),
                None => config::default_config_path()?,
            };
            let path = cfg_path.with_file_name("resize.toml");
            config::load_or_create_presets(&path, include_str!("../resize.toml")).unwrap_or_else(
                |e| {
                    eprintln!("warn: {e:#}");
                    Vec::new()
                },
            )
        } else {
            Vec::new()
        };
        run_one_shot(action, args.pid, presets)?;
        return Ok(());
    }

    let cfg_path = match args.config {
        Some(p) => p,
        None => config::default_config_path()?,
    };
    let cfg = match config::load(&cfg_path) {
        Ok(cfg) => cfg,
        Err(e) => {
            #[cfg(windows)]
            if args.plugin {
                if let Ok(link) = kit::ipc::PluginLink::new() {
                    let _ = link.notify("Window — config error", format!("{e:#}"));
                }
            }
            return Err(e);
        }
    };
    if cfg.bindings.is_empty() {
        anyhow::bail!("no usable bindings in {}", cfg_path.display());
    }

    #[cfg(windows)]
    win32::run_daemon(cfg_path, cfg, args.plugin)?;
    #[cfg(target_os = "linux")]
    run_daemon_linux(cfg_path, cfg)?;
    #[cfg(not(any(windows, target_os = "linux")))]
    compile_error!("window-tools supports Windows and Linux (X11) only");
    Ok(())
}

#[cfg(windows)]
fn run_one_shot(
    action: Action,
    pid: Option<u32>,
    presets: Vec<crate::layout::SizeSpec>,
) -> Result<()> {
    let p = win32::init();
    let mut app = platform::App::new(p);
    app.set_sizer(presets);
    let forced = pid.and_then(|pid| app.p.find_by_pid(pid));
    app.apply_to(action, forced)
}

#[cfg(target_os = "linux")]
fn run_one_shot(
    action: Action,
    pid: Option<u32>,
    presets: Vec<crate::layout::SizeSpec>,
) -> Result<()> {
    let p = x11::connect()?;
    let mut app = platform::App::new(p);
    app.set_sizer(presets);
    let forced = pid.and_then(|pid| app.p.find_by_pid(pid));
    app.apply_to(action, forced)
}

#[cfg(target_os = "linux")]
fn run_daemon_linux(cfg_path: std::path::PathBuf, mut cfg: config::Config) -> Result<()> {
    use std::time::{Duration, Instant};

    let mut x = x11::connect()?;
    let failed = x.grab_bindings(&cfg.bindings);
    if let Some(msg) = config::conflict_message(&failed) {
        eprintln!("warn: {msg}");
    }
    if x.grab_count() == 0 {
        anyhow::bail!("no hotkeys could be grabbed; exiting");
    }
    let mut app = platform::App::new(x);
    app.set_gap(cfg.settings.gap);
    let presets_path = cfg_path.with_file_name("resize.toml");
    let presets = config::load_or_create_presets(&presets_path, include_str!("../resize.toml"))
        .unwrap_or_else(|e| {
            eprintln!("warn: {e:#}");
            Vec::new()
        });
    app.set_sizer(presets);
    let mut mtime = config::modified(&cfg_path);
    let mut checked = Instant::now();

    loop {
        match app.p.poll_hotkey()? {
            Some(i) => {
                let action = cfg.bindings[i].action;
                if action == Action::Quit {
                    return Ok(());
                }
                if action == Action::SizerMenu {
                    match config::load_presets(&presets_path) {
                        Ok(p) if !p.is_empty() => app.set_sizer(p),
                        Ok(_) => {
                            eprintln!("warn: resize.toml has no usable presets; keeping old")
                        }
                        Err(e) => {
                            eprintln!("warn: cannot read resize.toml, keeping old presets: {e:#}")
                        }
                    }
                }
                if let Err(e) = app.apply(action) {
                    eprintln!("warn: {action:?}: {e:#}");
                }
            }
            None => std::thread::sleep(Duration::from_millis(16)),
        }

        if checked.elapsed() < Duration::from_secs(1) {
            continue;
        }
        checked = Instant::now();
        let now = config::modified(&cfg_path);
        if now == mtime {
            continue;
        }
        mtime = now;
        let new = match config::reload(&cfg_path) {
            Ok(new) if !new.bindings.is_empty() => new,
            Ok(_) => {
                eprintln!("warn: reloaded config has no usable bindings; keeping old config");
                continue;
            }
            Err(e) => {
                eprintln!("warn: config reload failed, keeping old config: {e:#}");
                continue;
            }
        };
        let old = std::mem::replace(&mut cfg, new);
        app.p.ungrab_all();
        let failed = app.p.grab_bindings(&cfg.bindings);
        if app.p.grab_count() == 0 {
            eprintln!("warn: reloaded config grabbed no hotkeys; keeping old config");
            cfg = old;
            app.p.ungrab_all();
            let _ = app.p.grab_bindings(&cfg.bindings);
            continue;
        }
        app.set_gap(cfg.settings.gap);
        if let Some(msg) = config::conflict_message(&failed) {
            eprintln!("warn: {msg}");
        }
    }
}
