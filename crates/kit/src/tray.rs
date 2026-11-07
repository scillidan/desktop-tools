//! Tray icon + menu + Win32 message loop, generic over all apps.
//!
//! Menu convention (from the AHK projects): a `>` prefix marks the active
//! state of a toggle item.
//!
//! One loop dispatches both menu clicks and [`Hotkeys`](crate::hotkey::Hotkeys)
//! events to a single app closure:

use crate::hotkey::Hotkeys;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use tray_icon::menu::{
    IsMenuItem, Menu, MenuEvent, MenuId, MenuItem, MenuItemBuilder, PredefinedMenuItem, Submenu,
};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, PostQuitMessage, PostThreadMessageW, MSG, WM_APP,
};

static TRAY_THREAD: AtomicU32 = AtomicU32::new(0);

/// Wake the tray loop from a helper thread. `PostMessageW(None, ...)` would
/// post to the *calling* thread's queue, so timer threads must target the loop
/// thread explicitly. The host uses reason `1` for its lifecycle tick.
pub fn post_wakeup(reason: usize) {
    let tid = TRAY_THREAD.load(Ordering::SeqCst);
    if tid == 0 {
        return;
    }
    unsafe {
        let _ = PostThreadMessageW(tid, WM_APP, WPARAM(reason), LPARAM(0));
    }
}

/// One menu row.
pub enum Entry {
    Sep,
    /// Clickable item with a stable label.
    Item(&'static str),
    /// Clickable item with a runtime label (counts, paths, plugin titles).
    ItemOwned(String),
    /// Clickable item with a stable id; `Event::MenuId` carries the id.
    ItemId(String, String),
    /// Toggle item; rendered with a `>` prefix when on.
    Toggle(&'static str, bool),
    /// Toggle item with a runtime label.
    ToggleOwned(String, bool),
    /// Toggle item with a stable id.
    ToggleId(String, String, bool),
    /// Nested menu with a stable label.
    Submenu(&'static str, Vec<Entry>),
    /// Nested menu with a runtime label.
    SubmenuOwned(String, Vec<Entry>),
    /// Disabled informational row (status, counts, errors).
    Info(String),
}

/// An event from the message loop.
pub enum Event {
    /// A menu item was clicked (`Item` or `Toggle` label).
    Menu(String),
    /// A menu item with a stable id was clicked.
    MenuId(String),
    /// A registered hotkey fired (its label).
    Hotkey(String),
    /// `WM_APP` posted to the loop's thread (state changed elsewhere, e.g. in a
    /// hook or timer thread). The `usize` is the message's `WPARAM.0` — apps
    /// use it to tell wakeup reasons apart.
    Wakeup(usize),
}

pub struct Tray {
    icon: TrayIcon,
    tooltip: String,
    ids: Vec<(MenuId, String)>,
    menu_ids: HashMap<MenuId, String>,
}

impl Tray {
    pub fn new(tooltip: &str, rgba: Vec<u8>) -> Result<Tray> {
        let icon = Icon::from_rgba(rgba, 32, 32).context("icon error")?;
        let (menu, ids, menu_ids) = build_menu(&[]);
        // `TrayIconBuilder::new()` consumes one counter value; the platform
        // icon then consumes a second for its internal uID. Starting from
        // `default()` (no counter use) and setting the id explicitly leaves
        // the first icon's internal uID at 1, which `notify()` uses.
        let icon = TrayIconBuilder::default()
            .with_id(tooltip)
            .with_menu(Box::new(menu))
            .with_tooltip(tooltip)
            .with_icon(icon)
            .build()
            .context("tray icon")?;
        Ok(Tray {
            icon,
            tooltip: tooltip.to_string(),
            ids,
            menu_ids,
        })
    }

    /// Re-render the menu. Call after app state changes (toggle flips).
    pub fn set_menu(&mut self, entries: &[Entry]) {
        let (menu, ids, menu_ids) = build_menu(entries);
        self.ids = ids;
        self.menu_ids = menu_ids;
        self.icon.set_menu(Some(Box::new(menu)));
    }

    pub fn set_tooltip(&mut self, text: &str) {
        self.tooltip = text.to_string();
        let _ = self.icon.set_tooltip(Some(&self.tooltip));
    }

    /// Swap the tray icon (e.g. locked/unlocked states).
    pub fn set_icon(&mut self, rgba: Vec<u8>) -> Result<()> {
        let icon = Icon::from_rgba(rgba, 32, 32).context("icon error")?;
        let _ = self.icon.set_icon(Some(icon));
        Ok(())
    }

    /// Show a system notification on this tray icon (a balloon, rendered as a
    /// toast by Windows 10/11). `tray-icon` does not expose its numeric id, so
    /// this assumes the first icon in the process (`uID = 1`); our apps create
    /// one.
    pub fn notify(&self, title: &str, body: &str) -> Result<()> {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::Shell::{
            Shell_NotifyIconW, NIF_INFO, NIIF_INFO, NIM_MODIFY, NOTIFYICONDATAW,
        };

        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: HWND(self.icon.window_handle()),
            uID: 1,
            uFlags: NIF_INFO,
            dwInfoFlags: NIIF_INFO,
            ..unsafe { std::mem::zeroed() }
        };
        copy_wide(&mut nid.szInfoTitle, title);
        copy_wide(&mut nid.szInfo, body);

        let ok = unsafe { Shell_NotifyIconW(NIM_MODIFY, &nid) };
        if !ok.as_bool() {
            anyhow::bail!("Shell_NotifyIconW(NIM_MODIFY) failed");
        }
        Ok(())
    }

    /// Stop [`run`](Tray::run) or [`run_headless`].
    pub fn quit(&self) {
        quit();
    }

    /// Message loop until quit. `on_event` receives menu, hotkey, and wakeup
    /// events. [`Event::Wakeup`] is fired when a `WM_APP` message is posted to
    /// this thread (e.g. from a hook proc that changed app state).
    pub fn run(&mut self, hotkeys: Option<&Hotkeys>, mut on_event: impl FnMut(&mut Tray, Event)) {
        unsafe {
            TRAY_THREAD.store(GetCurrentThreadId(), Ordering::SeqCst);
            let mut msg = MSG::default();
            loop {
                let ret = GetMessageW(&mut msg, None, 0, 0);
                if ret.0 <= 0 {
                    break;
                }
                DispatchMessageW(&msg);
                if msg.message == windows::Win32::UI::WindowsAndMessaging::WM_APP {
                    on_event(self, Event::Wakeup(msg.wParam.0));
                }
                if msg.message == windows::Win32::UI::WindowsAndMessaging::WM_HOTKEY {
                    if let Some(hotkeys) = hotkeys {
                        if let Some(label) = hotkeys.label_of(msg.wParam.0 as i32) {
                            on_event(self, Event::Hotkey(label.to_string()));
                        }
                    }
                }
                while let Some(event) = self.poll() {
                    on_event(self, event);
                }
            }
        }
    }

    /// Drain one pending menu event for an application-owned Win32 message loop.
    pub fn poll(&self) -> Option<Event> {
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if let Some(id) = self.menu_ids.get(&event.id) {
                return Some(Event::MenuId(id.clone()));
            }
            if let Some(label) = self.label_of(&event.id) {
                return Some(Event::Menu(label));
            }
        }
        None
    }

    fn label_of(&self, id: &MenuId) -> Option<String> {
        self.ids
            .iter()
            .find(|(mid, _)| mid == id)
            .map(|(_, label)| label.clone())
    }
}

fn copy_wide(dst: &mut [u16], text: &str) {
    let limit = dst.len().saturating_sub(1);
    for (slot, ch) in dst.iter_mut().take(limit).zip(text.encode_utf16()) {
        *slot = ch;
    }
}

/// Stop the current tray or headless message loop.
pub fn quit() {
    unsafe {
        PostQuitMessage(0);
    }
}

/// Message loop for hosted plugins: no `TrayIcon`, but the same hotkey and
/// `WM_APP` handling. Menu commands arrive as `post_wakeup(4)` wakeups.
pub fn run_headless(hotkeys: Option<&Hotkeys>, mut on_event: impl FnMut(Event)) {
    unsafe {
        TRAY_THREAD.store(GetCurrentThreadId(), Ordering::SeqCst);
        let mut msg = MSG::default();
        loop {
            let ret = GetMessageW(&mut msg, None, 0, 0);
            if ret.0 <= 0 {
                break;
            }
            DispatchMessageW(&msg);
            if msg.message == WM_APP {
                on_event(Event::Wakeup(msg.wParam.0));
            }
            if msg.message == windows::Win32::UI::WindowsAndMessaging::WM_HOTKEY {
                if let Some(hotkeys) = hotkeys {
                    if let Some(label) = hotkeys.label_of(msg.wParam.0 as i32) {
                        on_event(Event::Hotkey(label.to_string()));
                    }
                }
            }
        }
    }
}

fn build_menu(entries: &[Entry]) -> (Menu, Vec<(MenuId, String)>, HashMap<MenuId, String>) {
    let menu = Menu::new();
    let mut ids = Vec::new();
    let mut menu_ids = HashMap::new();
    push_entries(&Container::Menu(&menu), entries, &mut ids, &mut menu_ids);
    (menu, ids, menu_ids)
}

enum Container<'a> {
    Menu(&'a Menu),
    Submenu(&'a Submenu),
}

impl Container<'_> {
    fn append(&self, item: &dyn IsMenuItem) {
        match self {
            Container::Menu(menu) => {
                let _ = menu.append(item);
            }
            Container::Submenu(submenu) => {
                let _ = submenu.append(item);
            }
        }
    }
}

fn push_entries(
    container: &Container,
    entries: &[Entry],
    ids: &mut Vec<(MenuId, String)>,
    menu_ids: &mut HashMap<MenuId, String>,
) {
    for entry in entries {
        match entry {
            Entry::Sep => {
                container.append(&PredefinedMenuItem::separator());
            }
            Entry::Item(label) => {
                // muda's builder defaults to disabled, which greys out every row.
                let item = MenuItemBuilder::new().text(*label).enabled(true).build();
                ids.push((item.id().clone(), label.to_string()));
                container.append(&item);
            }
            Entry::Toggle(label, on) => {
                let text = if *on {
                    format!("> {label}")
                } else {
                    label.to_string()
                };
                let item: MenuItem = MenuItemBuilder::new().text(text).enabled(true).build();
                ids.push((item.id().clone(), label.to_string()));
                container.append(&item);
            }
            Entry::ItemOwned(label) => {
                let item = MenuItemBuilder::new()
                    .text(label.clone())
                    .enabled(true)
                    .build();
                ids.push((item.id().clone(), label.clone()));
                container.append(&item);
            }
            Entry::ItemId(id, label) => {
                let item = MenuItemBuilder::new()
                    .text(label.clone())
                    .enabled(true)
                    .build();
                ids.push((item.id().clone(), label.clone()));
                menu_ids.insert(item.id().clone(), id.clone());
                container.append(&item);
            }
            Entry::ToggleOwned(label, on) => {
                let text = if *on {
                    format!("> {label}")
                } else {
                    label.clone()
                };
                let item: MenuItem = MenuItemBuilder::new().text(text).enabled(true).build();
                ids.push((item.id().clone(), label.clone()));
                container.append(&item);
            }
            Entry::ToggleId(id, label, on) => {
                let text = if *on {
                    format!("> {label}")
                } else {
                    label.clone()
                };
                let item: MenuItem = MenuItemBuilder::new().text(text).enabled(true).build();
                ids.push((item.id().clone(), label.clone()));
                menu_ids.insert(item.id().clone(), id.clone());
                container.append(&item);
            }
            Entry::Submenu(label, children) => {
                let submenu = Submenu::new(*label, true);
                push_entries(&Container::Submenu(&submenu), children, ids, menu_ids);
                container.append(&submenu);
            }
            Entry::SubmenuOwned(label, children) => {
                let submenu = Submenu::new(label.clone(), true);
                push_entries(&Container::Submenu(&submenu), children, ids, menu_ids);
                container.append(&submenu);
            }
            Entry::Info(text) => {
                let item = MenuItemBuilder::new()
                    .text(text.clone())
                    .enabled(false)
                    .build();
                container.append(&item);
            }
        }
    }
}
