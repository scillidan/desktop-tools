//! Suite host: profile/core.toml, grouped plugin discovery, process lifecycle
//! and the single tray menu.

use crate::job::Job;
use anyhow::{Context, Result};
use kit::ipc::{HostCommand, PluginEvent, PROTOCOL_VERSION};
use kit::menu::{MenuItem, MenuModel};
use kit::tray::{Entry, Event, Tray};
use plugin::catalog::{Catalog, Plugin};
use plugin::core_config::CoreConfig;
use plugin::lifecycle::backoff;
use plugin::manifest::{group_index, GROUPS};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const APP: &str = "desktop-tools";
const LEGACY_AUTOSTART: &[&str] = &[
    "keyboard-tools-rs",
    "desktop-tools-rs",
    "window-tools-rs",
    "windowtools-rs",
    "rectangle-rs",
    "taskbar-tools-rs",
    "window-tools",
    "taskbar-tools",
];
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const DEFAULT_CORE: &str = include_str!("../core.toml");
const CORE_CHECK: Duration = Duration::from_secs(2);
const STABLE_RESET: Duration = Duration::from_secs(30);
const STOP_GRACE: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Status {
    Stopped,
    Running,
    Stopping,
    Restarting,
    Failed,
}

impl Status {
    fn label(self) -> &'static str {
        match self {
            Status::Stopped => "stopped",
            Status::Running => "running",
            Status::Stopping => "stopping",
            Status::Restarting => "restarting",
            Status::Failed => "failed",
        }
    }
}

fn suite_icon_state(has_error: bool, enabled_plugins: usize) -> kit::icon::State {
    if has_error {
        kit::icon::State::Error
    } else if enabled_plugins == 0 {
        kit::icon::State::Disabled
    } else {
        kit::icon::State::Normal
    }
}

struct Hosted {
    plugin: Plugin,
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    menu: MenuModel,
    ipc_status: Option<String>,
    status: Status,
    restarts: u32,
    next_start: Option<Instant>,
    stopping: bool,
    stop_started: Option<Instant>,
    restart_after_stop: bool,
    started_at: Option<Instant>,
}

struct Host {
    job: Job,
    profile: PathBuf,
    plugins_root: PathBuf,
    core_path: PathBuf,
    core: CoreConfig,
    core_mtime: Option<SystemTime>,
    last_core_check: Instant,
    plugins: Vec<Hosted>,
    catalog_errors: Vec<String>,
    log_dir: PathBuf,
    events_tx: Sender<(String, Result<PluginEvent, String>)>,
    events_rx: Receiver<(String, Result<PluginEvent, String>)>,
    menu_targets: HashMap<String, (String, String)>,
    shutting_down: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PluginAction {
    Enable,
    Disable,
    Restart,
    Edit,
    Log,
}

impl Host {
    fn new(profile: PathBuf, plugins_root: PathBuf) -> Result<Host> {
        std::fs::create_dir_all(profile.join("logs"))
            .with_context(|| format!("cannot create {}", profile.join("logs").display()))?;
        let exe_dir = std::env::current_exe()
            .context("cannot locate own executable")?
            .parent()
            .context("host executable has no parent")?
            .to_path_buf();
        kit::config::write_reference_in_profile(&profile, "core.toml", DEFAULT_CORE)?;
        let (core_path, text) =
            kit::config::load_in_profile(&profile, &exe_dir, "core.toml", DEFAULT_CORE)?;
        let core = CoreConfig::parse(&text).context("core.toml")?;
        let mut catalog = Catalog::discover(&plugins_root);
        let catalog_errors = std::mem::take(&mut catalog.errors);
        let plugins = catalog
            .plugins
            .into_iter()
            .map(|plugin| Hosted {
                plugin,
                child: None,
                stdin: None,
                menu: MenuModel::default(),
                ipc_status: None,
                status: Status::Stopped,
                restarts: 0,
                next_start: None,
                stopping: false,
                stop_started: None,
                restart_after_stop: false,
                started_at: None,
            })
            .collect();
        let (events_tx, events_rx) = mpsc::channel();
        let mut host = Host {
            job: Job::new()?,
            log_dir: profile.join("logs"),
            profile,
            plugins_root,
            core_path,
            core,
            core_mtime: None,
            last_core_check: Instant::now(),
            plugins,
            catalog_errors,
            events_tx,
            events_rx,
            menu_targets: HashMap::new(),
            shutting_down: false,
        };
        for error in &host.catalog_errors {
            host.log(format!("invalid plugin: {error}"));
        }
        host.core_mtime = std::fs::metadata(&host.core_path)
            .ok()
            .and_then(|m| m.modified().ok());
        let desired_autostart = host.core.host.start_with_windows;
        if let Err(e) = sync_autostart(desired_autostart) {
            host.log(format!("cannot sync autostart: {e:#}"));
        }
        host.apply_enabled();
        Ok(host)
    }

    fn log(&self, msg: impl AsRef<str>) {
        use std::io::Write;
        let path = self.log_dir.join("host.log");
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(file, "{}", msg.as_ref());
        }
    }

    fn index_of(&self, id: &str) -> Option<usize> {
        self.plugins.iter().position(|p| p.plugin.manifest.id == id)
    }

    fn refresh_menu(&mut self, tray: &mut Tray) {
        let enabled = self.core.enabled_set();
        let mut grouped: Vec<Vec<Entry>> = GROUPS.iter().map(|_| Vec::new()).collect();
        let mut group_sizes = [0usize; GROUPS.len()];
        let mut group_status: [Option<String>; GROUPS.len()] = std::array::from_fn(|_| None);
        for hosted in &self.plugins {
            group_sizes[group_index(&hosted.plugin.manifest.group).unwrap()] += 1;
        }
        let mut targets = HashMap::new();
        for hosted in &self.plugins {
            let id = &hosted.plugin.manifest.id;
            let group = group_index(&hosted.plugin.manifest.group)
                .expect("catalog accepts only known groups");
            let inline = group_sizes[group] == 1;
            if inline {
                group_status[group] = Some(hosted.status.label().to_string());
            }
            let mut sub = plugin_menu_header(hosted.plugin.manifest.description.as_deref(), inline);
            let contributed = render_plugin_items(id, &hosted.menu, &mut targets);
            let has_contributed = !contributed.is_empty();
            sub.extend(contributed);
            if has_contributed {
                sub.push(Entry::Sep);
            }
            if enabled.contains(id) {
                sub.push(Entry::ItemOwned(format!("Disable {id}")));
            } else {
                sub.push(Entry::ItemOwned(format!("Enable {id}")));
            }
            sub.push(Entry::ItemOwned(format!("Restart {id}")));
            if hosted.plugin.manifest.config {
                sub.push(Entry::ItemOwned(format!("Edit {id} config")));
            }
            sub.push(Entry::ItemOwned(format!("Open {id} log")));
            push_group_entry(
                &mut grouped[group],
                &hosted.plugin.manifest.name,
                hosted.status.label(),
                sub,
                inline,
            );
        }
        self.menu_targets = targets;
        let mut entries = Vec::new();
        for (index, ((_, title), items)) in GROUPS.iter().zip(grouped).enumerate() {
            if !items.is_empty() {
                entries.push(Entry::SubmenuOwned(
                    group_title(title, group_sizes[index], group_status[index].as_deref()),
                    items,
                ));
            }
        }
        if entries.is_empty() {
            entries.push(Entry::ItemOwned(format!(
                "No plugins found at {}",
                self.plugins_root.display()
            )));
        }
        if !self.catalog_errors.is_empty() {
            entries.push(Entry::Sep);
            entries.push(Entry::Info(format!(
                "Invalid packages: {} (see host.log)",
                self.catalog_errors.len()
            )));
        }

        entries.push(Entry::Sep);
        entries.push(Entry::Item("Reload core"));
        entries.push(Entry::Item("Edit core.toml"));
        entries.push(Entry::Toggle(
            "Start with Windows",
            kit::win::autostart_enabled(APP),
        ));
        entries.push(Entry::Item("Open Profile Folder"));
        entries.push(Entry::Item("About"));
        entries.push(Entry::Item("Exit"));
        tray.set_menu(&entries);
        let enabled_plugins = self
            .plugins
            .iter()
            .filter(|hosted| enabled.contains(&hosted.plugin.manifest.id))
            .count();
        let has_error = !self.catalog_errors.is_empty()
            || self
                .plugins
                .iter()
                .any(|hosted| hosted.status == Status::Failed);
        let _ = tray.set_icon(kit::icon::rgba(
            kit::icon::DESKTOP,
            suite_icon_state(has_error, enabled_plugins),
        ));
    }

    fn edit_profile_file(&self, name: &str) {
        let user = self.profile.join(name);
        if !user.exists() {
            kit::win::message_box(
                APP,
                &format!(
                    "{} does not exist yet.\nStart the plugin once, then edit it.",
                    user.display()
                ),
            );
            return;
        }
        if let Err(error) = kit::win::edit_profile_config(&user) {
            kit::win::message_box(APP, &format!("Cannot open {name}:\n{error:#}"));
        }
    }

    fn on_menu(&mut self, tray: &mut Tray, label: &str) {
        match label {
            "Reload core" => {
                self.reload_core(true);
                self.refresh_menu(tray);
                return;
            }
            "Edit core.toml" => {
                self.edit_profile_file("core.toml");
                return;
            }
            "Start with Windows" => {
                let now = !kit::win::autostart_enabled(APP);
                self.core.host.start_with_windows = now;
                if let Err(e) = self.persist_core() {
                    kit::win::message_box(APP, &format!("Cannot save core.toml:\n{e:#}"));
                }
                if let Err(e) = sync_autostart(now) {
                    kit::win::message_box(APP, &format!("Autostart failed:\n{e:#}"));
                }
                self.refresh_menu(tray);
                return;
            }
            "Open Profile Folder" => {
                kit::win::open_folder(&self.profile);
                return;
            }
            "About" => {
                kit::win::info_box(
                    APP,
                    &format!(
                        "desktop-tools {}\n\nProfile:\n{}\n\nPlugins:\n{}\n\n{} plugins, {} enabled",
                        env!("CARGO_PKG_VERSION"),
                        self.profile.display(),
                        self.plugins_root.display(),
                        self.plugins.len(),
                        self.core.plugins.enabled.len()
                    ),
                );
                return;
            }
            _ => {}
        }

        let Some((id, action)) = parse_plugin_label(label) else {
            return;
        };
        let Some(idx) = self.index_of(id) else {
            return;
        };
        match action {
            PluginAction::Enable => {
                self.core.set_enabled(id, true);
                if let Err(e) = self.persist_core() {
                    kit::win::message_box(APP, &format!("Cannot save core.toml:\n{e:#}"));
                }
                self.start_index(idx, true);
            }
            PluginAction::Disable => {
                self.core.set_enabled(id, false);
                if let Err(e) = self.persist_core() {
                    kit::win::message_box(APP, &format!("Cannot save core.toml:\n{e:#}"));
                }
                self.plugins[idx].restart_after_stop = false;
                self.stop_index(idx);
            }
            PluginAction::Restart => {
                self.core.set_enabled(id, true);
                if let Err(e) = self.persist_core() {
                    kit::win::message_box(APP, &format!("Cannot save core.toml:\n{e:#}"));
                }
                if self.plugins[idx].child.is_some() {
                    self.stop_index(idx);
                    self.plugins[idx].restart_after_stop = true;
                } else {
                    self.start_index(idx, true);
                }
            }
            PluginAction::Edit => self.edit_profile_file(&format!("{id}.toml")),
            PluginAction::Log => {
                let path = self.log_dir.join(format!("{id}.log"));
                if path.exists() {
                    kit::win::edit_config(&path);
                } else {
                    kit::win::message_box(APP, &format!("No log yet:\n{}", path.display()));
                }
            }
        }
        self.refresh_menu(tray);
    }

    fn on_menu_id(&mut self, namespaced: &str) {
        let Some((plugin_id, action_id)) = self.menu_targets.get(namespaced).cloned() else {
            return;
        };
        let Some(idx) = self.index_of(&plugin_id) else {
            return;
        };
        let Some(stdin) = self.plugins[idx].stdin.as_mut() else {
            self.log(format!(
                "{plugin_id}: invoke `{action_id}` without a running child"
            ));
            return;
        };
        let command = HostCommand::Invoke {
            v: PROTOCOL_VERSION,
            id: action_id.clone(),
        };
        match command.to_line() {
            Ok(line) => {
                if let Err(e) = stdin.write_all(line.as_bytes()).and_then(|_| stdin.flush()) {
                    self.log(format!("{plugin_id}: cannot write invoke: {e:#}"));
                }
            }
            Err(e) => self.log(format!("{plugin_id}: cannot encode invoke: {e:#}")),
        }
    }

    fn tick(&mut self, tray: &Tray) -> bool {
        let now = Instant::now();
        let mut changed = self.drain_events(tray);

        for i in 0..self.plugins.len() {
            let mut exited = false;
            let wait = self.plugins[i]
                .child
                .as_mut()
                .map(|child| (child.id(), child.try_wait()));
            if let Some((pid, wait)) = wait {
                match wait {
                    Ok(Some(_)) => exited = true,
                    Ok(None) => {
                        if self.plugins[i].restarts > 0
                            && self.plugins[i]
                                .started_at
                                .is_some_and(|t| now.duration_since(t) >= STABLE_RESET)
                        {
                            self.plugins[i].restarts = 0;
                        }
                    }
                    Err(e) => self.log(format!(
                        "{}: try_wait failed: {e:#}",
                        self.plugins[i].plugin.manifest.id
                    )),
                }
                let stop_elapsed = self.plugins[i]
                    .stop_started
                    .map(|started| now.duration_since(started))
                    .unwrap_or_default();
                if self.plugins[i].stopping && stop_step(exited, stop_elapsed) == StopStep::Force {
                    let id = &self.plugins[i].plugin.manifest.id;
                    self.log(format!(
                        "{id}: graceful stop timed out; killing process tree"
                    ));
                    kill_tree(pid);
                    if let Some(child) = self.plugins[i].child.as_mut() {
                        let _ = child.wait();
                    }
                    exited = true;
                }
            }
            if exited {
                self.plugins[i].child = None;
                self.plugins[i].stdin = None;
                self.plugins[i].menu = MenuModel::default();
                self.plugins[i].ipc_status = None;
                changed = true;
                let id = self.plugins[i].plugin.manifest.id.clone();
                if self.plugins[i].stopping {
                    let restart = self.plugins[i].restart_after_stop && !self.shutting_down;
                    self.plugins[i].stopping = false;
                    self.plugins[i].stop_started = None;
                    self.plugins[i].restart_after_stop = false;
                    self.plugins[i].restarts = 0;
                    self.plugins[i].started_at = None;
                    if restart {
                        self.plugins[i].status = Status::Restarting;
                        self.plugins[i].next_start = Some(now);
                    } else {
                        self.plugins[i].status = Status::Stopped;
                        self.plugins[i].next_start = None;
                    }
                    self.log(format!("{id}: stopped"));
                } else {
                    self.plugins[i].restarts += 1;
                    let attempt = self.plugins[i].restarts.saturating_sub(1);
                    if let Some(delay) = backoff(attempt) {
                        self.plugins[i].status = Status::Restarting;
                        self.plugins[i].next_start = Some(now + delay);
                        self.log(format!(
                            "{id}: exited unexpectedly, restart {}/3 in {:?}",
                            self.plugins[i].restarts, delay
                        ));
                    } else {
                        self.plugins[i].status = Status::Failed;
                        self.plugins[i].next_start = None;
                        self.log(format!("{id}: failed after 3 restarts"));
                    }
                }
            }
        }

        let enabled = self.core.enabled_set();
        for i in 0..self.plugins.len() {
            let id = self.plugins[i].plugin.manifest.id.clone();
            if !self.shutting_down
                && self.plugins[i].child.is_none()
                && self.plugins[i].status == Status::Restarting
                && self.plugins[i].next_start.is_some_and(|t| t <= now)
                && enabled.contains(&id)
            {
                self.start_index(i, false);
                changed = true;
            }
        }

        if now.duration_since(self.last_core_check) >= CORE_CHECK {
            self.last_core_check = now;
            if self.core_changed() {
                changed |= self.reload_core(false);
            }
        }
        changed
    }

    fn drain_events(&mut self, tray: &Tray) -> bool {
        let mut changed = false;
        while let Ok((id, event)) = self.events_rx.try_recv() {
            match event {
                Err(e) => self.log(format!("{id}: invalid IPC line: {e}")),
                Ok(PluginEvent::Ready { v: _, menu, status }) => {
                    if let Err(e) = menu.validate() {
                        self.log(format!("{id}: invalid menu: {e:#}"));
                    } else if let Some(idx) = self.index_of(&id) {
                        self.log(format!(
                            "{id}: ready menu_items={} status={:?}",
                            menu.items.len(),
                            status
                        ));
                        self.plugins[idx].menu = menu;
                        self.plugins[idx].ipc_status = status;
                        changed = true;
                    }
                }
                Ok(PluginEvent::Menu { v: _, items }) => {
                    if let Err(e) = items.validate() {
                        self.log(format!("{id}: invalid menu update: {e:#}"));
                    } else if let Some(idx) = self.index_of(&id) {
                        self.log(format!("{id}: menu update items={}", items.items.len()));
                        self.plugins[idx].menu = items;
                        changed = true;
                    }
                }
                Ok(PluginEvent::Status { v: _, text }) => {
                    if let Some(idx) = self.index_of(&id) {
                        self.plugins[idx].ipc_status = Some(text);
                        changed = true;
                    }
                }
                Ok(PluginEvent::Notify { v: _, title, body }) => {
                    if let Err(e) = tray.notify(&title, &body) {
                        self.log(format!("{id}: notification failed: {e:#}"));
                    }
                }
                Ok(PluginEvent::Error { v: _, message }) => {
                    self.log(format!("{id}: error: {message}"));
                }
                Ok(PluginEvent::Log {
                    v: _,
                    level,
                    message,
                }) => {
                    self.log(format!("{id}: [{level}] {message}"));
                }
            }
        }
        changed
    }

    fn core_changed(&self) -> bool {
        std::fs::metadata(&self.core_path)
            .ok()
            .and_then(|m| m.modified().ok())
            != self.core_mtime
    }

    fn reload_core(&mut self, show_error: bool) -> bool {
        let text = match std::fs::read_to_string(&self.core_path) {
            Ok(text) => text,
            Err(e) => {
                self.log(format!("cannot read core.toml: {e:#}"));
                if show_error {
                    kit::win::message_box(APP, &format!("Cannot read core.toml:\n{e:#}"));
                }
                return false;
            }
        };
        let core = match CoreConfig::parse(&text) {
            Ok(core) => core,
            Err(e) => {
                self.log(format!("invalid core.toml: {e:#}"));
                if show_error {
                    kit::win::message_box(APP, &format!("Invalid core.toml:\n{e:#}"));
                }
                return false;
            }
        };
        self.core = core;
        self.core_mtime = std::fs::metadata(&self.core_path)
            .ok()
            .and_then(|m| m.modified().ok());
        self.log("core.toml reloaded");
        self.apply_enabled();
        true
    }

    fn apply_enabled(&mut self) {
        if self.shutting_down {
            return;
        }
        let enabled = self.core.enabled_set();
        let catalog_ids: HashSet<&str> = self
            .plugins
            .iter()
            .map(|p| p.plugin.manifest.id.as_str())
            .collect();
        for id in &enabled {
            if !catalog_ids.contains(id.as_str()) {
                self.log(format!("enabled id `{id}` is not installed"));
            }
        }
        for i in 0..self.plugins.len() {
            let id = self.plugins[i].plugin.manifest.id.clone();
            let want = enabled.contains(&id);
            let running = self.plugins[i].child.is_some();
            let status = self.plugins[i].status;
            if want && !running && status != Status::Restarting {
                self.start_index(i, true);
            } else if !want && (running || status != Status::Stopped) {
                self.stop_index(i);
            }
        }
    }

    fn start_index(&mut self, idx: usize, reset: bool) {
        if self.shutting_down {
            return;
        }
        if reset {
            self.plugins[idx].restarts = 0;
        }
        let plugin = &self.plugins[idx].plugin;
        let result = spawn_plugin(
            plugin,
            &self.profile,
            &self.log_dir,
            &self.job,
            &self.events_tx,
        );
        match result {
            Ok((child, stdin)) => {
                self.log(format!(
                    "{}: started pid {}",
                    plugin.manifest.id,
                    child.id()
                ));
                let hosted = &mut self.plugins[idx];
                hosted.child = Some(child);
                hosted.stdin = Some(stdin);
                hosted.menu = MenuModel::default();
                hosted.ipc_status = None;
                hosted.status = Status::Running;
                hosted.next_start = None;
                hosted.stopping = false;
                hosted.stop_started = None;
                hosted.restart_after_stop = false;
                hosted.started_at = Some(Instant::now());
            }
            Err(e) => {
                let id = plugin.manifest.id.clone();
                self.log(format!("{id}: cannot start: {e:#}"));
                let hosted = &mut self.plugins[idx];
                hosted.child = None;
                hosted.stdin = None;
                hosted.menu = MenuModel::default();
                hosted.ipc_status = None;
                hosted.restarts += 1;
                let attempt = hosted.restarts.saturating_sub(1);
                if let Some(delay) = backoff(attempt) {
                    hosted.status = Status::Restarting;
                    hosted.next_start = Some(Instant::now() + delay);
                } else {
                    hosted.status = Status::Failed;
                    hosted.next_start = None;
                }
            }
        }
    }

    fn stop_index(&mut self, idx: usize) {
        let id = self.plugins[idx].plugin.manifest.id.clone();
        let hosted = &mut self.plugins[idx];
        if hosted.stopping {
            return;
        }
        hosted.next_start = None;
        hosted.restart_after_stop = false;
        if hosted.child.is_none() {
            hosted.status = Status::Stopped;
            hosted.stopping = false;
            hosted.stop_started = None;
            hosted.restarts = 0;
            hosted.started_at = None;
            hosted.menu = MenuModel::default();
            hosted.ipc_status = None;
            return;
        }
        hosted.stopping = true;
        hosted.stop_started = Some(Instant::now());
        hosted.status = Status::Stopping;
        hosted.menu = MenuModel::default();
        hosted.ipc_status = None;
        if let Some(mut stdin) = hosted.stdin.take() {
            let quit = HostCommand::Quit {
                v: PROTOCOL_VERSION,
            };
            if let Ok(line) = quit.to_line() {
                let _ = stdin.write_all(line.as_bytes());
                let _ = stdin.flush();
            }
        }
        self.log(format!("{id}: graceful stop requested"));
    }

    fn stop_all(&mut self) {
        self.shutting_down = true;
        for i in 0..self.plugins.len() {
            self.plugins[i].restart_after_stop = false;
            if self.plugins[i].child.is_some() || self.plugins[i].status != Status::Stopped {
                self.stop_index(i);
            }
        }
    }

    fn all_stopped(&self) -> bool {
        self.plugins.iter().all(|plugin| plugin.child.is_none())
    }

    fn force_stop_all(&mut self) {
        for hosted in &mut self.plugins {
            if let Some(mut child) = hosted.child.take() {
                kill_tree(child.id());
                let _ = child.wait();
            }
            hosted.stdin = None;
            hosted.status = Status::Stopped;
            hosted.stopping = false;
            hosted.stop_started = None;
            hosted.restart_after_stop = false;
            hosted.next_start = None;
            hosted.started_at = None;
            hosted.menu = MenuModel::default();
            hosted.ipc_status = None;
        }
    }

    fn persist_core(&mut self) -> Result<()> {
        let text = self.core.to_toml()?;
        std::fs::write(&self.core_path, text)
            .with_context(|| format!("cannot write {}", self.core_path.display()))?;
        self.core_mtime = std::fs::metadata(&self.core_path)
            .ok()
            .and_then(|m| m.modified().ok());
        Ok(())
    }
}

fn group_title(title: &str, size: usize, status: Option<&str>) -> String {
    match (size, status) {
        (1, Some(status)) => format!("{title} ({status})"),
        _ => title.to_string(),
    }
}

fn plugin_menu_header(description: Option<&str>, inline: bool) -> Vec<Entry> {
    match (inline, description) {
        (false, Some(description)) => vec![Entry::Info(description.into()), Entry::Sep],
        _ => Vec::new(),
    }
}

fn push_group_entry(
    entries: &mut Vec<Entry>,
    name: &str,
    status: &str,
    plugin_entries: Vec<Entry>,
    inline: bool,
) {
    if inline {
        entries.extend(plugin_entries);
    } else {
        entries.push(Entry::SubmenuOwned(
            format!("{name} ({status})"),
            plugin_entries,
        ));
    }
}

fn render_plugin_items(
    plugin_id: &str,
    model: &MenuModel,
    targets: &mut HashMap<String, (String, String)>,
) -> Vec<Entry> {
    if model.validate().is_err() {
        return vec![Entry::Info("Invalid plugin menu model".into())];
    }
    render_plugin_item_slice(plugin_id, &model.items, targets)
}

fn render_plugin_item_slice(
    plugin_id: &str,
    items: &[MenuItem],
    targets: &mut HashMap<String, (String, String)>,
) -> Vec<Entry> {
    items
        .iter()
        .map(|item| match item {
            MenuItem::Separator => Entry::Sep,
            MenuItem::Info { label } => Entry::Info(label.clone()),
            MenuItem::Submenu { label, items } => Entry::SubmenuOwned(
                label.clone(),
                render_plugin_item_slice(plugin_id, items, targets),
            ),
            MenuItem::Item {
                id,
                label,
                enabled: true,
            } => {
                let namespaced = format!("{plugin_id}::{id}");
                targets.insert(namespaced.clone(), (plugin_id.to_string(), id.clone()));
                Entry::ItemId(namespaced, label.clone())
            }
            MenuItem::Toggle {
                id,
                label,
                checked,
                enabled: true,
            }
            | MenuItem::Radio {
                id,
                label,
                checked,
                enabled: true,
            } => {
                let namespaced = format!("{plugin_id}::{id}");
                targets.insert(namespaced.clone(), (plugin_id.to_string(), id.clone()));
                Entry::ToggleId(namespaced, label.clone(), *checked)
            }
            MenuItem::Item { label, .. }
            | MenuItem::Toggle { label, .. }
            | MenuItem::Radio { label, .. } => Entry::Info(label.clone()),
        })
        .collect()
}

fn parse_plugin_label(label: &str) -> Option<(&str, PluginAction)> {
    if let Some(id) = label.strip_prefix("Enable ") {
        return Some((id, PluginAction::Enable));
    }
    if let Some(id) = label.strip_prefix("Disable ") {
        return Some((id, PluginAction::Disable));
    }
    if let Some(id) = label.strip_prefix("Restart ") {
        return Some((id, PluginAction::Restart));
    }
    if let Some(id) = label.strip_prefix("Edit ") {
        return id
            .strip_suffix(" config")
            .map(|id| (id, PluginAction::Edit));
    }
    if let Some(id) = label.strip_prefix("Open ") {
        return id.strip_suffix(" log").map(|id| (id, PluginAction::Log));
    }
    None
}

fn spawn_plugin(
    plugin: &Plugin,
    profile: &Path,
    log_dir: &Path,
    job: &Job,
    events_tx: &Sender<(String, Result<PluginEvent, String>)>,
) -> Result<(Child, ChildStdin)> {
    let log_path = log_dir.join(format!("{}.log", plugin.manifest.id));
    let mut log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("cannot open {}", log_path.display()))?;
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    writeln!(log, "\n--- {} start {secs} ---", plugin.manifest.id)?;

    let mut cmd = Command::new(plugin.dir.join(&plugin.manifest.exe));
    cmd.arg("--profile")
        .arg(profile)
        .arg("--plugin")
        .arg("--no-tray")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(log));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd
        .spawn()
        .with_context(|| format!("cannot start {}", plugin.manifest.exe))?;
    if let Err(e) = job.assign(&child) {
        let _ = child.kill();
        return Err(e).context("cannot assign plugin to the kill-on-close job");
    }
    let stdin = child.stdin.take().context("plugin stdin")?;
    let stdout = child.stdout.take().context("plugin stdout")?;
    let id = plugin.manifest.id.clone();
    let tx = events_tx.clone();
    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            let event = match line {
                Ok(line) => PluginEvent::from_line(&line).map_err(|e| format!("{e:#}")),
                Err(e) => Err(format!("read stdout: {e:#}")),
            };
            if tx.send((id.clone(), event)).is_err() {
                break;
            }
            kit::tray::post_wakeup(2);
        }
    });
    Ok((child, stdin))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StopStep {
    Wait,
    Exited,
    Force,
}

fn stop_step(exited: bool, elapsed: Duration) -> StopStep {
    if exited {
        StopStep::Exited
    } else if elapsed >= STOP_GRACE {
        StopStep::Force
    } else {
        StopStep::Wait
    }
}

fn sync_autostart(desired: bool) -> Result<()> {
    if desired {
        for legacy in LEGACY_AUTOSTART {
            if kit::win::autostart_enabled(legacy) {
                kit::win::set_autostart(legacy, false)?;
            }
        }
    }
    if desired != kit::win::autostart_enabled(APP) {
        kit::win::set_autostart(APP, desired)?;
    }
    Ok(())
}

#[cfg(windows)]
fn kill_tree(pid: u32) {
    use std::os::windows::process::CommandExt;
    let _ = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .creation_flags(CREATE_NO_WINDOW)
        .status();
}

fn plugins_root() -> Result<PathBuf> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--plugins" {
            return Ok(PathBuf::from(
                args.get(i + 1).context("--plugins requires a directory")?,
            ));
        }
        if let Some(dir) = args[i].strip_prefix("--plugins=") {
            if dir.is_empty() {
                anyhow::bail!("--plugins requires a directory");
            }
            return Ok(PathBuf::from(dir));
        }
        i += 1;
    }
    let exe = std::env::current_exe().context("cannot locate own executable")?;
    Ok(exe
        .parent()
        .context("host executable has no parent")?
        .join("plugins"))
}

pub fn run() -> Result<()> {
    if std::env::args().any(|arg| arg == "--version" || arg == "-V") {
        println!("desktop-tools {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let _single = kit::win::ensure_single_instance(APP)?;
    let profile = kit::config::profile_dir()?;
    std::fs::create_dir_all(&profile)
        .with_context(|| format!("cannot create {}", profile.display()))?;
    let root = plugins_root()?;
    let mut host = Host::new(profile, root)?;
    let mut tray = Tray::new(
        APP,
        kit::icon::rgba(kit::icon::DESKTOP, kit::icon::State::Normal),
    )?;
    host.refresh_menu(&mut tray);

    std::thread::spawn(|| loop {
        std::thread::sleep(Duration::from_millis(500));
        kit::tray::post_wakeup(1);
    });

    let mut exit_deadline = None;
    tray.run(None, |tray, event| match event {
        Event::Menu(label) if label == "Exit" && exit_deadline.is_none() => {
            host.stop_all();
            host.refresh_menu(tray);
            exit_deadline = Some(Instant::now() + STOP_GRACE + Duration::from_secs(1));
            if host.all_stopped() {
                tray.quit();
            }
        }
        Event::Menu(label) if exit_deadline.is_none() => host.on_menu(tray, &label),
        Event::MenuId(id) if exit_deadline.is_none() => host.on_menu_id(&id),
        Event::Wakeup(_) => {
            if host.tick(tray) {
                host.refresh_menu(tray);
            }
            if let Some(deadline) = exit_deadline {
                if host.all_stopped() {
                    tray.quit();
                } else if Instant::now() >= deadline {
                    host.force_stop_all();
                    tray.quit();
                }
            }
        }
        _ => {}
    });
    host.force_stop_all();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_profile_enables_every_shipped_plugin() {
        let core = CoreConfig::parse(DEFAULT_CORE).unwrap();
        let enabled = core.enabled_set();
        let expected = [
            "window-tools",
            "screen-tools",
            "taskbar-tools",
            "keyboard-keymap",
            "keyboard-auto-switch-back",
            "keyboard-locker",
            "keyboard-switch",
            "keyboard-shell",
            "goldendict-auto-search",
            "goldendict-search-in-group",
        ];
        assert_eq!(enabled.len(), expected.len());
        for id in expected {
            assert!(enabled.contains(id), "fresh profile must enable {id}");
        }
    }

    #[test]
    fn suite_icon_reflects_errors_and_disabled_state() {
        assert_eq!(suite_icon_state(false, 1), kit::icon::State::Normal);
        assert_eq!(suite_icon_state(true, 1), kit::icon::State::Error);
        assert_eq!(suite_icon_state(false, 0), kit::icon::State::Disabled);
    }

    fn host_with_running_child() -> Host {
        use std::os::windows::process::CommandExt;
        let root = std::env::temp_dir().join(format!("keyboard-host-test-{}", std::process::id()));
        let log_dir = root.join("logs");
        std::fs::create_dir_all(&log_dir).unwrap();
        let mut child = Command::new("cmd")
            .args(["/C", "ping -n 10 127.0.0.1 >NUL"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let (events_tx, events_rx) = mpsc::channel();
        let hosted = Hosted {
            plugin: Plugin {
                manifest: plugin::manifest::Manifest {
                    id: "stop-test".into(),
                    name: "stop-test".into(),
                    version: "1".into(),
                    api: 1,
                    exe: "unused.exe".into(),
                    group: "keyboard".into(),
                    optional: false,
                    external: false,
                    description: None,
                    config: true,
                    tray: "host".into(),
                },
                dir: root.clone(),
            },
            child: Some(child),
            stdin: Some(stdin),
            menu: MenuModel::default(),
            ipc_status: None,
            status: Status::Running,
            restarts: 0,
            next_start: None,
            stopping: false,
            stop_started: None,
            restart_after_stop: false,
            started_at: Some(Instant::now()),
        };
        Host {
            job: Job::new().unwrap(),
            profile: root.clone(),
            plugins_root: root.clone(),
            core_path: root.join("core.toml"),
            core: CoreConfig::default(),
            core_mtime: None,
            last_core_check: Instant::now(),
            plugins: vec![hosted],
            catalog_errors: Vec::new(),
            log_dir,
            events_tx,
            events_rx,
            menu_targets: HashMap::new(),
            shutting_down: false,
        }
    }

    #[test]
    fn plugin_menu_labels_are_parsed_by_id() {
        assert_eq!(
            parse_plugin_label("Enable keyboard-locker"),
            Some(("keyboard-locker", PluginAction::Enable))
        );
        assert_eq!(
            parse_plugin_label("Disable keyboard-switch"),
            Some(("keyboard-switch", PluginAction::Disable))
        );
        assert_eq!(
            parse_plugin_label("Edit goldendict-auto-search config"),
            Some(("goldendict-auto-search", PluginAction::Edit))
        );
        assert_eq!(
            parse_plugin_label("Open keyboard-locker log"),
            Some(("keyboard-locker", PluginAction::Log))
        );
        assert_eq!(parse_plugin_label("Reload core"), None);
    }

    #[test]
    fn nested_plugin_menu_keeps_namespaced_targets() {
        let model = MenuModel::new(vec![MenuItem::Submenu {
            label: "Rectangle".into(),
            items: vec![MenuItem::Item {
                id: "unpin".into(),
                label: "Unpin all".into(),
                enabled: true,
            }],
        }]);
        let mut targets = HashMap::new();
        let entries = render_plugin_items("window-tools", &model, &mut targets);
        assert!(
            matches!(entries.as_slice(), [Entry::SubmenuOwned(label, _)] if label == "Rectangle")
        );
        assert_eq!(
            targets.get("window-tools::unpin"),
            Some(&("window-tools".into(), "unpin".into()))
        );
    }

    #[test]
    fn single_plugin_groups_inline_and_multi_plugin_groups_keep_wrappers() {
        assert_eq!(
            group_title("Window", 1, Some("running")),
            "Window (running)"
        );
        assert_eq!(group_title("Keyboard", 5, None), "Keyboard");
        let plugin_entries = || {
            vec![Entry::ItemId(
                "window-tools::reload".into(),
                "Reload".into(),
            )]
        };
        let mut single = Vec::new();
        push_group_entry(&mut single, "Window", "running", plugin_entries(), true);
        assert!(matches!(single.as_slice(), [Entry::ItemId(_, label)] if label == "Reload"));

        let mut multi = Vec::new();
        push_group_entry(&mut multi, "Window", "running", plugin_entries(), false);
        assert!(
            matches!(multi.as_slice(), [Entry::SubmenuOwned(label, _)] if label == "Window (running)")
        );
    }

    #[test]
    fn nested_plugin_header_is_description_only() {
        assert!(plugin_menu_header(None, false).is_empty());
        assert!(plugin_menu_header(Some("ignored for inline groups"), true).is_empty());
        let entries = plugin_menu_header(Some("Replaces exact text triggers."), false);
        assert_eq!(entries.len(), 2);
        assert!(
            matches!(&entries[0], Entry::Info(label) if label == "Replaces exact text triggers.")
        );
        assert!(matches!(entries[1], Entry::Sep));
    }

    #[test]
    fn graceful_stop_waits_then_times_out() {
        assert_eq!(stop_step(false, std::time::Duration::ZERO), StopStep::Wait);
        assert_eq!(stop_step(false, STOP_GRACE), StopStep::Force);
        assert_eq!(stop_step(true, std::time::Duration::ZERO), StopStep::Exited);
        assert_eq!(stop_step(true, STOP_GRACE), StopStep::Exited);
    }

    #[test]
    fn stop_request_retains_child_for_graceful_reap() {
        let mut host = host_with_running_child();
        host.stop_index(0);
        assert!(host.plugins[0].child.is_some());
        assert!(host.plugins[0].stopping);
        assert_eq!(host.plugins[0].status, Status::Stopping);
        assert!(host.plugins[0].stop_started.is_some());
        host.force_stop_all();
        assert!(host.all_stopped());
    }

    #[test]
    fn restart_from_disabled_enables_core_and_process() {
        let mut host = host_with_running_child();
        host.force_stop_all();
        host.core.set_enabled("stop-test", false);
        host.plugins[0].plugin.manifest.exe = std::env::var("COMSPEC").unwrap();
        let icon = kit::icon::rgba(kit::icon::DESKTOP, kit::icon::State::Normal);
        let mut tray = Tray::new("keyboard-tools-test", icon).unwrap();

        host.on_menu(&mut tray, "Restart stop-test");

        assert_eq!(host.plugins[0].status, Status::Running);
        assert!(host.core.enabled_set().contains("stop-test"));
        let saved = CoreConfig::parse(&std::fs::read_to_string(&host.core_path).unwrap()).unwrap();
        assert!(saved.enabled_set().contains("stop-test"));
        host.force_stop_all();
    }
}
