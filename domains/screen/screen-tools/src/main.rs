//! screen-tools-rs — OCR frame window + overlay (Text-Grab model).
//!
//! P0.2: a borderless always-on-top hollow frame marks a fixed OCR region.
//! Draw it with the draw binding, resize it by typed SizeSpec, pin it
//! click-through, and capture its region to the clipboard. OCR lands in P1.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod capture;
mod config;
mod config_files;
mod desktops;
mod flameshot;
mod frame;
mod host;
mod hotkeys;
mod icon;
mod mask;
mod menu;
mod ocr;
mod overlay;
mod tray;

use anyhow::{Context, Result};
use std::io::Write;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::time::SystemTime;

use config::Action;
pub use screen_draw::frame_hwnd;
use screen_draw::{
    cleanup_temp_captures, pin_style, set_capture_dir, set_external_editor, set_pin_style,
};
use screen_record as record;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwareness, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE,
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, PROCESS_DPI_AWARENESS,
};

static HOST_HWND: AtomicIsize = AtomicIsize::new(0);
const WM_HANDOFF_START: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 72;
const WM_HANDOFF_END: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 73;

fn post_handoff_state(end: bool) {
    let hwnd = HOST_HWND.load(Ordering::Acquire);
    if hwnd != 0 {
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                Some(HWND(hwnd as *mut _)),
                if end {
                    WM_HANDOFF_END
                } else {
                    WM_HANDOFF_START
                },
                windows::Win32::Foundation::WPARAM(0),
                windows::Win32::Foundation::LPARAM(0),
            );
        }
    }
}

fn handoff_started() {
    post_handoff_state(false);
}

fn handoff_finished() {
    post_handoff_state(true);
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            // `{:#}` is anyhow's Display chain, not Debug.
            eprintln!("error: {e:#}");
            logline(&format!("error: {e:#}"));
            std::process::ExitCode::FAILURE
        }
    }
}

fn set_dpi_aware() {
    // Provenance: desktop-tools-rs src/win32.rs set_dpi_aware.
    unsafe {
        for ctx in [
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE,
        ] {
            if SetProcessDpiAwarenessContext(ctx).is_ok() {
                return;
            }
        }
        // Fallback for pre-Win10: system-DPI-aware beats nothing.
        let _ = SetProcessDpiAwareness(PROCESS_DPI_AWARENESS(1));
    }
}

pub(crate) fn ocr_frame_ops() -> screen_ocr::FrameOps {
    screen_draw::ocr_frame_ops()
}

fn frame_settings(cfg: &config::Config) -> screen_draw::FrameSettings {
    screen_draw::FrameSettings {
        frame_w: cfg.frame_w,
        frame_h: cfg.frame_h,
        work_area: cfg.work_area,
        show_hint: cfg.show_hint,
        font: cfg.font.clone(),
        ocr_scale: cfg.ocr_scale,
        ocr_language: cfg.ocr_language.clone(),
    }
}

fn run() -> Result<()> {
    let hosted = std::env::args().any(|arg| arg == "--plugin");
    set_dpi_aware();
    flameshot_handoff::set_logger(logline);
    screen_capture::set_logger(logline);
    screen_draw::set_logger(logline);
    screen_draw::set_notifier(tray::notify);
    screen_ocr::set_logger(logline);
    screen_record::set_logger(logline);
    unsafe {
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
    }
    // One resident daemon only. Several instances each own a frame and cannot
    // register the same hotkeys; quitting the tray of one leaves the others
    // running, which looks like the app refuses to exit.
    let _single = unsafe {
        use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
        use windows::Win32::System::Threading::CreateMutexW;
        match CreateMutexW(None, true, windows::core::w!("Local\\screen-tools.single")) {
            Ok(handle) => {
                if GetLastError() == ERROR_ALREADY_EXISTS {
                    logline("another screen-tools instance is already running; exiting");
                    return Ok(());
                }
                Some(handle)
            }
            Err(e) => {
                logline(&format!("warn: single-instance mutex unavailable: {e}"));
                None
            }
        }
    };
    // Hold the legacy mutex too, so old and renamed executables cannot run together.
    let _legacy_single = unsafe {
        use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
        use windows::Win32::System::Threading::CreateMutexW;
        match CreateMutexW(
            None,
            true,
            windows::core::w!("Local\\screen-tools-rs.single"),
        ) {
            Ok(handle) => {
                if GetLastError() == ERROR_ALREADY_EXISTS {
                    logline("legacy screen-tools-rs instance is already running; exiting");
                    return Ok(());
                }
                Some(handle)
            }
            Err(e) => {
                logline(&format!(
                    "warn: legacy single-instance mutex unavailable: {e}"
                ));
                None
            }
        }
    };
    let cfg_path = kit::config::profile_file(config_files::ACTIONS_FILE)?;
    let legacy_path = kit::config::profile_file(config::CONFIG_NAME)?;
    let exe = std::env::current_exe().context("cannot locate own executable")?;
    let legacy_dir = exe.parent().context("executable has no parent directory")?;
    kit::config::import_legacy_once(&legacy_path, legacy_dir, &["dt-ocr.toml"])?;
    let mut cfg = config_files::load(&cfg_path)?;
    ocr::set_engine(cfg.ocr_engine);
    ocr::set_auto(cfg.ocr_auto);
    mask::set_fill(cfg.mask_fill);
    mask::set_style(cfg.mask_font.clone(), cfg.mask_size, cfg.mask_mode);
    set_capture_dir(&cfg.capture_dir);
    tray::set_notify(cfg.notify_enabled);
    flameshot::set_path(cfg.flameshot.clone());
    set_external_editor(&cfg.external_editor);
    set_record_cfg(&cfg);
    cleanup_temp_captures();
    set_pin_style(cfg.pin_border, cfg.pin_line_color, cfg.pin_hatch_color);
    logline(&format!(
        "config loaded from {} (frame {}x{})",
        cfg_path.display(),
        cfg.frame_w,
        cfg.frame_h
    ));

    let host = host::Host::new()?;
    HOST_HWND.store(host.hwnd().0 as isize, Ordering::Release);
    flameshot::set_session_hooks(handoff_started, handoff_finished);
    let plugin = if hosted {
        let hwnd = host.hwnd().0 as isize;
        let link = kit::ipc::PluginLink::new_with_wakeup(move |_| unsafe {
            let hwnd = HWND(hwnd as *mut core::ffi::c_void);
            let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                Some(hwnd),
                windows::Win32::UI::WindowsAndMessaging::WM_APP + 70,
                windows::Win32::Foundation::WPARAM(0),
                windows::Win32::Foundation::LPARAM(0),
            );
        })?;
        tray::set_notifier(link.notifier());
        link.ready(
            screen_menu(false, false),
            Some("screen-tools: running".into()),
        )?;
        Some(link)
    } else {
        None
    };
    let mut frames = desktops::Frames::new(&frame_settings(&cfg))?;
    let _tray = if hosted {
        None
    } else {
        tray::set_menu_model(screen_menu(false, true));
        match tray::Tray::new(host.hwnd()) {
            Ok(tray) => Some(tray),
            Err(e) => {
                logline(&format!("warn: tray unavailable: {e:#}"));
                None
            }
        }
    };
    let mut suspended = false;
    let mut handoff_active = false;

    let mut hotkeys = register_hotkeys(host.hwnd(), &cfg, false);
    let mut mtime = config_files::modified(&cfg_path);

    // 1 s timer drives config hot-reload (bindings only; the desktop-tools-rs
    // pattern — mtime polling, no notify dependency).
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::{SetTimer, TIMERPROC};
        let _ = SetTimer(Some(host.hwnd()), 1, 250, Some(TIMERPROC::None));
    }

    let mut msg = windows::Win32::UI::WindowsAndMessaging::MSG::default();
    let mut last_config_poll = std::time::Instant::now();
    loop {
        let got =
            unsafe { windows::Win32::UI::WindowsAndMessaging::GetMessageW(&mut msg, None, 0, 0) };
        if !got.as_bool() {
            break; // WM_QUIT
        }
        if msg.message == WM_HANDOFF_START {
            handoff_active = true;
            hotkeys.unregister_for_handoff(host.hwnd());
            logline("external: Screen hotkeys released (Power retained)");
        }
        if msg.message == WM_HANDOFF_END {
            handoff_active = false;
            hotkeys.unregister_all(host.hwnd());
            if !suspended {
                hotkeys = register_hotkeys(host.hwnd(), &cfg, false);
            }
            logline("external: Screen hotkeys restored");
        }
        if msg.message == windows::Win32::UI::WindowsAndMessaging::WM_APP + 70 {
            if let Some(link) = &plugin {
                while let Some(command) = link.try_recv() {
                    match command {
                        kit::ipc::HostCommand::Invoke { id, .. } if id == "suspend" => {
                            toggle_suspended(
                                &mut suspended,
                                &mut hotkeys,
                                &host,
                                &cfg,
                                handoff_active,
                                Some(link),
                            );
                            logline(&format!("screen group suspended={suspended}"));
                        }
                        kit::ipc::HostCommand::Reload { .. } => {
                            mtime = None;
                            last_config_poll =
                                std::time::Instant::now() - std::time::Duration::from_secs(1);
                            unsafe {
                                let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                                    Some(host.hwnd()),
                                    windows::Win32::UI::WindowsAndMessaging::WM_TIMER,
                                    windows::Win32::Foundation::WPARAM(1),
                                    windows::Win32::Foundation::LPARAM(0),
                                );
                            }
                        }
                        kit::ipc::HostCommand::Quit { .. } => unsafe {
                            windows::Win32::UI::WindowsAndMessaging::PostQuitMessage(0)
                        },
                        kit::ipc::HostCommand::Invoke { id, .. } => {
                            if let Some(module) = config_files::MODULES
                                .iter()
                                .find(|module| id == format!("config-{}", module.id()))
                            {
                                config_files::edit(*module, &cfg_path);
                            } else {
                                logline("warn: unknown host menu command");
                            }
                        }
                    }
                }
            }
        }
        if msg.message == menu::WM_MENU_COMMAND {
            if let Some(id) = menu::command_id(msg.wParam.0) {
                match id {
                    "suspend" => {
                        toggle_suspended(
                            &mut suspended,
                            &mut hotkeys,
                            &host,
                            &cfg,
                            handoff_active,
                            None,
                        );
                        logline(&format!("screen group suspended={suspended}"));
                    }
                    "quit" => unsafe {
                        windows::Win32::UI::WindowsAndMessaging::PostQuitMessage(0)
                    },
                    _ => {
                        if let Some(module) = config_files::MODULES
                            .iter()
                            .find(|module| id == format!("config-{}", module.id()))
                        {
                            config_files::edit(*module, &cfg_path);
                        }
                    }
                }
            }
        }
        if msg.message == windows::Win32::UI::WindowsAndMessaging::WM_HOTKEY {
            if let Some(action) = hotkeys.action_of(msg.wParam.0 as i32) {
                // MOD_NOREPEAT already suppresses held-key repeats; this also
                // swallows duplicate injected messages before they stack pins.
                let mut last = LAST_HOTKEY
                    .lock()
                    .unwrap_or_else(|poison| poison.into_inner());
                let repeated = hotkey_repeat(&mut last, action, std::time::Instant::now());
                drop(last);
                if repeated {
                    logline(&format!("hotkey repeat ignored: {action:?}"));
                    continue;
                }
                if action == config::Action::Draw && overlay::is_visible() {
                    logline("draw already active; hotkey ignored");
                    continue;
                }
                if action == config::Action::Draw && overlay::recently_finished() {
                    logline("draw completion repeat ignored");
                    continue;
                }
                logline(&format!("hotkey: {action:?}"));
                if action == Action::ToggleScreen {
                    screen_power::toggle();
                    continue;
                }
                match frames.current(&frame_settings(&cfg)) {
                    Ok(current) => dispatch(action, current),
                    Err(e) => logline(&format!("warn: cannot select desktop frame: {e:#}")),
                }
            }
        }
        if msg.message == windows::Win32::UI::WindowsAndMessaging::WM_TIMER
            && msg.wParam.0 == 1
            && last_config_poll.elapsed() >= std::time::Duration::from_secs(1)
        {
            last_config_poll = std::time::Instant::now();
            let now = config_files::modified(&cfg_path);
            if now != mtime {
                mtime = now;
                match reload(&cfg_path) {
                    Ok(new_cfg) => {
                        logline("config reloaded, hotkeys re-registered");
                        ocr::set_engine(new_cfg.ocr_engine);
                        ocr::set_auto(new_cfg.ocr_auto);
                        hotkeys.unregister_all(host.hwnd());
                        if !suspended {
                            hotkeys = register_hotkeys(host.hwnd(), &new_cfg, handoff_active);
                        }
                        for &current in frames.all() {
                            unsafe {
                                frame::set_prefs(
                                    current,
                                    new_cfg.work_area,
                                    new_cfg.show_hint,
                                    new_cfg.font.clone(),
                                    new_cfg.ocr_scale,
                                    new_cfg.ocr_language.clone(),
                                )
                            };
                        }
                        mask::set_fill(new_cfg.mask_fill);
                        set_capture_dir(&new_cfg.capture_dir);
                        tray::set_notify(new_cfg.notify_enabled);
                        flameshot::set_path(new_cfg.flameshot.clone());
                        set_external_editor(&new_cfg.external_editor);
                        set_record_cfg(&new_cfg);
                        set_pin_style(
                            new_cfg.pin_border,
                            new_cfg.pin_line_color,
                            new_cfg.pin_hatch_color,
                        );
                        mask::set_style(
                            new_cfg.mask_font.clone(),
                            new_cfg.mask_size,
                            new_cfg.mask_mode,
                        );
                        cfg = new_cfg;
                        tray::set_state(if suspended {
                            kit::icon::State::Suspended
                        } else {
                            kit::icon::State::Normal
                        });
                    }
                    Err(e) => {
                        logline(&format!("config reload failed, keeping old: {e:#}"));
                        tray::set_state(kit::icon::State::Error);
                    }
                }
            }
        }
        unsafe {
            if msg.message == windows::Win32::UI::WindowsAndMessaging::WM_TIMER && msg.wParam.0 == 1
            {
                let visible = frames
                    .all()
                    .iter()
                    .copied()
                    .find(|&h| frame::visible_rect(h).is_some());
                if let Some(frame) = visible {
                    ocr::auto_tick(frame, mask::dismiss, frame::prepare_ocr, ocr_frame_ops());
                }
            }
            if msg.message == windows::Win32::UI::WindowsAndMessaging::WM_TIMER {
                screen_power::tick();
            }
            let _ = windows::Win32::UI::WindowsAndMessaging::TranslateMessage(&msg);
            windows::Win32::UI::WindowsAndMessaging::DispatchMessageW(&msg);
        }
    }
    hotkeys.unregister_all(host.hwnd());
    // Do not leave an ffmpeg child recording after the daemon exits.
    if record::is_recording() {
        if let Err(e) = record::stop() {
            logline(&format!("record stop on exit failed: {e:#}"));
        }
    }
    drop(frames);
    unsafe { windows::Win32::System::Com::CoUninitialize() };
    logline("message loop exited");
    Ok(())
}

fn screen_menu(suspended: bool, standalone: bool) -> kit::menu::MenuModel {
    use kit::menu::MenuItem;
    let mut items: Vec<MenuItem> = config_files::MODULES
        .iter()
        .map(|&module| MenuItem::Submenu {
            label: module.label().into(),
            items: vec![
                MenuItem::Info {
                    label: module.description().into(),
                },
                MenuItem::Separator,
                MenuItem::Item {
                    id: format!("config-{}", module.id()),
                    label: format!("Edit {} config", module.label()),
                    enabled: true,
                },
            ],
        })
        .collect();
    items.push(MenuItem::Separator);
    items.push(MenuItem::Toggle {
        id: "suspend".into(),
        label: "Suspend Group \"Screen\"".into(),
        checked: suspended,
        enabled: true,
    });
    if standalone {
        items.push(MenuItem::Item {
            id: "quit".into(),
            label: "Exit Screen Tools".into(),
            enabled: true,
        });
    }
    kit::menu::MenuModel::new(items)
}

fn toggle_suspended(
    suspended: &mut bool,
    hotkeys: &mut hotkeys::Hotkeys,
    host: &host::Host,
    cfg: &config::Config,
    handoff_active: bool,
    link: Option<&kit::ipc::PluginLink>,
) {
    *suspended = !*suspended;
    if *suspended {
        hotkeys.unregister_all(host.hwnd());
        ocr::disable_auto();
        unsafe { mask::dismiss() };
    } else {
        *hotkeys = register_hotkeys(host.hwnd(), cfg, handoff_active);
    }
    if let Some(link) = link {
        let _ = link.menu(&screen_menu(*suspended, false));
    } else {
        tray::set_menu_model(screen_menu(*suspended, true));
    }
    tray::set_state(if *suspended {
        kit::icon::State::Suspended
    } else {
        kit::icon::State::Normal
    });
}

fn register_hotkeys(hwnd: HWND, cfg: &config::Config, handoff_active: bool) -> hotkeys::Hotkeys {
    let (hk, failed) = if handoff_active {
        hotkeys::Hotkeys::register_for_handoff(hwnd, &cfg.bindings)
    } else {
        hotkeys::Hotkeys::register(hwnd, &cfg.bindings)
    };
    logline(&format!("hotkeys registered: {}", hk.len()));
    for f in &failed {
        logline(&format!("warn: hotkey not registered: {f}"));
    }
    if hk.is_empty() {
        logline("warn: no hotkeys registered; running frame-only");
    }
    hk
}

/// Hot-reload re-reads the file; an unreadable or binding-less result keeps
/// the old config, mirroring the desktop-tools-rs daemon.
fn reload(cfg_path: &std::path::Path) -> Result<config::Config> {
    config_files::reload(cfg_path)
}

fn allowed_during_handoff(action: Action, active: bool) -> bool {
    !active || matches!(action, Action::ToggleScreen)
}

fn dispatch(action: Action, frame: HWND) {
    // A recording owns the screen: our own overlays (mask, dim, a moved frame)
    // must not appear in the take, so only Record may run until it stops.
    if crate::record::is_recording() && !matches!(action, Action::Record) {
        logline(&format!(
            "{} ignored: recording (stop it first)",
            action.name()
        ));
        return;
    }
    if !allowed_during_handoff(action, flameshot_handoff::is_active()) {
        logline(&format!(
            "{} ignored: Flameshot handoff (exit it first)",
            action.name()
        ));
        return;
    }
    let visible = unsafe { frame::visible_rect(frame).is_some() };
    // Only the commands that create or place a frame may run without one.
    if !visible
        && matches!(
            action,
            Action::Resize | Action::Pin | Action::Capture | Action::Move | Action::Toggle
        )
    {
        logline(&format!(
            "{} ignored: no frame (use draw or frame_window)",
            action.name()
        ));
        return;
    }
    unsafe {
        match action {
            Action::Draw => {
                frame::hide(frame);
                mask::dismiss();
                if let Err(e) = overlay::begin(overlay::Intent::Frame) {
                    logline(&format!("draw failed: {e:#}"));
                }
            }
            Action::QuickOcr => {
                frame::hide(frame);
                mask::dismiss();
                if let Err(e) = overlay::begin(overlay::Intent::QuickOcr) {
                    logline(&format!("quick OCR draw failed: {e:#}"));
                }
            }
            Action::AutoOcr => {
                // Second press ends the session: no more refreshes, frame gone.
                if ocr::auto_enabled() {
                    if ocr::auto_paused() {
                        mask::dismiss();
                        frame::prepare_ocr(frame, ocr::Intent::Auto, true);
                        ocr::resume_auto(frame);
                        if let Err(e) = ocr::start(frame, ocr_frame_ops()) {
                            logline(&format!("auto OCR resume failed: {e:#}"));
                            frame::clear_flash(frame);
                            ocr::disable_auto();
                        } else {
                            logline("auto OCR: resumed");
                        }
                        return;
                    }
                    ocr::disable_auto();
                    mask::dismiss();
                    frame::hide(frame);
                    logline("auto OCR: off");
                    return;
                }
                if !visible {
                    if let Err(e) = overlay::begin(overlay::Intent::AutoOcr) {
                        logline(&format!("auto OCR draw failed: {e:#}"));
                    }
                    return;
                }
                mask::dismiss();
                frame::prepare_ocr(frame, ocr::Intent::Auto, true);
                ocr::enable_auto(frame);
                if let Err(e) = ocr::start(frame, ocr_frame_ops()) {
                    logline(&format!("auto OCR failed: {e:#}"));
                    frame::clear_flash(frame);
                    ocr::disable_auto();
                }
                logline("auto OCR: on");
            }
            Action::Resize => {
                // Pause any OCR session and drop an in-flight result: the
                // user is adjusting the frame, not reading it.
                mask::dismiss();
                frame::pause_ocr(frame);
                frame::ensure_visible(frame);
                frame::begin_spec(frame);
            }
            Action::Pin => {
                frame::ensure_visible(frame);
                frame::toggle_pin(frame);
            }
            Action::Move => {
                frame::ensure_visible(frame);
                frame::to_move(frame);
            }
            Action::Toggle => mask::toggle(),
            Action::ToggleScreen => screen_power::toggle(),
            // Ctrl+Alt+Z: fullscreen DWM-thumbnail zoom of the focused window.
            // Ctrl+Alt+R: record the frame rect, or stop the running session.
            // The frame stays on screen with a red REC border, but the capture
            // rect is inset past that border so the red edge never lands in the
            // video.
            Action::Record => {
                if record::is_recording() {
                    if let Err(e) = record::stop() {
                        logline(&format!("record stop failed: {e:#}"));
                    }
                    frame::set_recording(frame, false);
                    frame::ensure_visible(frame);
                    frame::focus(frame);
                } else {
                    match frame::visible_rect(frame) {
                        Some(rect) => {
                            let inner = kit::layout::inset(frame::rect_of(rect), 2);
                            let settings = record_cfg();
                            match record::start(inner, &settings) {
                                Ok(()) => {
                                    // Our own overlays must not end up in the
                                    // take: drop the OCR result and keep OCR
                                    // from popping up again while it runs.
                                    mask::dismiss();
                                    logline(
                                        "record: dismissed OCR result; OCR paused while recording",
                                    );
                                    frame::ensure_visible(frame);
                                    frame::set_recording(frame, true);
                                }
                                Err(e) => {
                                    logline(&format!("record failed: {e:#}"));
                                    frame::set_recording(frame, false);
                                    frame::ensure_visible(frame);
                                }
                            }
                        }
                        None => logline("record ignored: no frame"),
                    }
                }
            }
            Action::Capture => {
                // Read the real screen: a previous mask would be re-captured.
                mask::dismiss();
                if let Err(e) = capture::capture_frame(frame) {
                    logline(&format!("capture failed: {e:#}"));
                } else {
                    frame::flash_capture(frame);
                    frame::prepare_ocr(frame, ocr::Intent::Manual, true);
                    // P0.3: copy the image (above) AND read the words into the
                    // overlay. OCR runs on a worker thread; the frame is
                    // click-through until the mask takes over.
                    if let Err(e) = ocr::start(frame, ocr_frame_ops()) {
                        logline(&format!("ocr capture failed: {e:#}"));
                    }
                }
            }
            // Frame the window the user is looking at: the frame becomes the
            // region for OCR/capture without a manual drag.
            Action::FrameWindow => {
                mask::dismiss();
                match capture::window_target() {
                    Ok(rect) => {
                        frame::place(frame, frame::rect_of(rect));
                        frame::to_move(frame);
                        // Re-targeting keeps an armed auto session armed; it
                        // only restarts the comparison against the new region.
                        ocr::retarget_auto(frame);
                        logline(&format!("frame: window at {rect:?}"));
                    }
                    Err(e) => logline(&format!("frame window failed: {e:#}")),
                }
            }
            // Pin what the frame covers; without a frame, pin the focused
            // window (this action intentionally needs no frame).
            Action::PinShot => {
                let r = match frame::visible_rect(frame) {
                    Some(rect) => {
                        // Hide first: our own border must not be in the image.
                        frame::hide(frame);
                        let r = capture::pin_rect(rect, pin_style());
                        if r.is_err() {
                            frame::ensure_visible(frame);
                        }
                        r
                    }
                    None => capture::pin_window(pin_style()),
                };
                if let Err(e) = r {
                    logline(&format!("pin shot failed: {e:#}"));
                }
            }
        }
    }
}

static LAST_HOTKEY: std::sync::Mutex<Option<(Action, std::time::Instant)>> =
    std::sync::Mutex::new(None);

/// `MOD_NOREPEAT` covers a held key; this also drops duplicate injected
/// `WM_HOTKEY`s that arrive back-to-back and would stack capture/pin windows.
fn hotkey_repeat(
    last: &mut Option<(Action, std::time::Instant)>,
    action: Action,
    now: std::time::Instant,
) -> bool {
    let gate_ms = if action == Action::Draw { 500 } else { 150 };
    if let Some((prev, at)) = *last {
        if prev == action && now.duration_since(at).as_millis() < gate_ms {
            return true;
        }
    }
    *last = Some((action, now));
    false
}

static RECORD_CFG: std::sync::Mutex<record::Settings> = std::sync::Mutex::new(record::Settings {
    dir: String::new(),
    name: String::new(),
    fps: 30,
    ffmpeg: String::new(),
    audio: true,
    crf: 20,
    max_minutes: 60,
    min_free_mb: 500,
});

fn set_record_cfg(cfg: &config::Config) {
    if let Ok(mut c) = RECORD_CFG.lock() {
        *c = record::Settings {
            dir: cfg.record_dir.clone(),
            name: cfg.record_name.clone(),
            fps: cfg.record_fps,
            ffmpeg: cfg.record_ffmpeg.clone(),
            audio: cfg.record_audio,
            crf: cfg.record_crf,
            max_minutes: cfg.record_max_minutes,
            min_free_mb: cfg.record_min_free_mb,
        };
    }
}

fn record_cfg() -> record::Settings {
    RECORD_CFG
        .lock()
        .map(|c| c.clone())
        .unwrap_or_else(|e| e.into_inner().clone())
}

/// Append-only profile log (release builds have no console).
fn logline(msg: &str) {
    let Ok(dir) = kit::config::profile_dir().map(|p| p.join("logs")) else {
        return;
    };
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join("screen-tools.log");
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let secs = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let _ = writeln!(f, "[{secs}] {msg}");
    }
}

#[cfg(test)]
mod tests {
    use super::{allowed_during_handoff, hotkey_repeat};
    use crate::config::Action;
    use std::time::{Duration, Instant};

    #[test]
    fn handoff_blocks_screen_actions_except_monitor_power() {
        for action in Action::ALL {
            assert!(allowed_during_handoff(action, false));
            assert_eq!(
                allowed_during_handoff(action, true),
                matches!(action, Action::ToggleScreen),
                "{action:?}"
            );
        }
    }

    #[test]
    fn duplicate_hotkeys_inside_the_gate_are_dropped() {
        let mut last = None;
        let t = Instant::now();
        assert!(!hotkey_repeat(&mut last, Action::PinShot, t));
        assert!(hotkey_repeat(
            &mut last,
            Action::PinShot,
            t + Duration::from_millis(40)
        ));
        assert!(!hotkey_repeat(
            &mut last,
            Action::PinShot,
            t + Duration::from_millis(200)
        ));
        // A different action is never treated as a repeat.
        assert!(!hotkey_repeat(
            &mut last,
            Action::Draw,
            t + Duration::from_millis(210)
        ));
        // Physical Draw repetitions were observed about 300 ms after pointer
        // release; they must not hide the frame and restart the overlay.
        assert!(hotkey_repeat(
            &mut last,
            Action::Draw,
            t + Duration::from_millis(500) // 290 ms after the preceding event.
        ));
        assert!(!hotkey_repeat(
            &mut last,
            Action::Draw,
            t + Duration::from_millis(1010)
        ));
    }
}
