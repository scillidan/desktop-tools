#![cfg_attr(windows, windows_subsystem = "windows")]

//! keyboard-locker: press the shortcut (default Ctrl+Alt+Shift+Insert) to lock
//! the keyboard — a `WH_KEYBOARD_LL` hook swallows every key event. The same
//! combo unlocks, or type the password (default `unlock`). While locked, mouse
//! buttons are swallowed but movement still works. Clean-room MIT rewrite of
//! sophice/ahk-keyboard-locker.
//!
//! The tray has no locked/unlocked state; the suite host will own the only
//! tray. It is kept as a minimal control surface (config/reload/exit) until
//! the host arrives.

fn main() -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        if let Err(e) = run() {
            kit::win::fatal_error("keyboard-locker", &format!("Error:\n{e:#}"));
        }
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        kit::unimplemented("keyboard-locker")
    }
}

#[cfg(windows)]
mod lock {
    use anyhow::{bail, Context, Result};
    use std::sync::atomic::{AtomicBool, Ordering};
    use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows::Win32::UI::Input::KeyboardAndMouse::*;
    use windows::Win32::UI::WindowsAndMessaging::*;

    pub static LOCKED: AtomicBool = AtomicBool::new(false);
    /// Password mode: while locked, the toggle chord is swallowed instead of
    /// unlocking. Set `unlock_with_shortcut = true` for the old behavior.
    static UNLOCK_WITH_SHORTCUT: AtomicBool = AtomicBool::new(false);
    /// False right after locking, until the toggle chord's modifiers are
    /// released. Without this a fast typist's first password character sees
    /// Shift/Ctrl still down and is rejected.
    static PASSWORD_ARMED: AtomicBool = AtomicBool::new(false);

    /// Opt-in hook trace: `KBTOOLS_DEBUG=1` appends to
    /// `%TEMP%\keyboard-locker-hook.log`. Logs VK codes, never characters.
    pub(super) fn debug_log(msg: impl AsRef<str>) {
        if std::env::var_os("KBTOOLS_DEBUG").is_none() {
            return;
        }
        use std::io::Write;
        let path = std::env::temp_dir().join("keyboard-locker-hook.log");
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(file, "{}", msg.as_ref());
        }
    }

    /// Modifier/key state tracked from the events we see (not
    /// GetAsyncKeyState: swallowed events never reach the queue, so the
    /// async state would go stale while locked).
    #[derive(Default, Clone, Copy)]
    pub(super) struct Mods {
        ctrl: bool,
        shift: bool,
        alt: bool,
        win: bool,
        ralt: bool,
    }

    #[derive(Debug, PartialEq, Eq)]
    pub(super) enum Action {
        Toggle,
        Swallow,
        Pass,
    }

    /// Decide what the hook does with one event. Pure so the matching rules
    /// (exact modifiers, AltGr chord, locked swallowing) are unit-testable.
    pub(super) fn decide(
        combo: &kit::keys::Combo,
        m: &mut Mods,
        vk: VIRTUAL_KEY,
        down: bool,
        locked: bool,
    ) -> Action {
        match vk {
            VK_LCONTROL | VK_RCONTROL | VK_CONTROL => m.ctrl = down,
            VK_LSHIFT | VK_RSHIFT | VK_SHIFT => m.shift = down,
            VK_LMENU | VK_MENU => m.alt = down,
            VK_RMENU => {
                m.alt = down;
                m.ralt = down;
            }
            VK_LWIN | VK_RWIN => m.win = down,
            _ => {}
        }

        if !down {
            // Let modifier releases through while locked: their downs may
            // have been swallowed before the lock (or by the toggle chord),
            // and a stuck Ctrl/Alt/Shift/Win is worse than a lone key-up.
            return if locked && !is_modifier(vk) {
                Action::Swallow
            } else {
                Action::Pass
            };
        }

        let want = kit::hotkey::vk_of_key(combo.key);
        if want == Some(vk) && matches(combo, m) {
            Action::Toggle
        } else if locked {
            Action::Swallow
        } else {
            Action::Pass
        }
    }

    fn any_modifier(m: &Mods) -> bool {
        m.ctrl || m.shift || m.alt || m.win
    }

    fn is_modifier(vk: VIRTUAL_KEY) -> bool {
        matches!(
            vk,
            VK_LCONTROL
                | VK_RCONTROL
                | VK_CONTROL
                | VK_LSHIFT
                | VK_RSHIFT
                | VK_SHIFT
                | VK_LMENU
                | VK_RMENU
                | VK_MENU
                | VK_LWIN
                | VK_RWIN
        )
    }

    fn matches(combo: &kit::keys::Combo, m: &Mods) -> bool {
        if combo.altgr {
            // AltGr is reported as LCtrl+RAlt; the parser canonicalizes the
            // combo to altgr-only, so compare the chord, not ctrl/alt.
            m.ctrl && m.alt && m.ralt && m.shift == combo.shift && m.win == combo.win
        } else {
            m.ctrl == combo.ctrl
                && m.shift == combo.shift
                && m.alt == combo.alt
                && m.win == combo.win
                // A Ctrl+RAlt chord is AltGr, not a plain Ctrl+Alt combo.
                && !(m.ctrl && m.ralt)
        }
    }

    #[derive(Clone, Copy)]
    struct PasswordChar {
        vk: VIRTUAL_KEY,
        shift: bool,
    }

    /// A typed key sequence. The characters are mapped to VKs once, with the
    /// current layout; `down` suppresses auto-repeat so holding `u` does not
    /// advance the password twice.
    pub struct Password {
        chars: Vec<PasswordChar>,
        progress: usize,
        down: [bool; 256],
    }

    impl Password {
        pub fn new(text: &str) -> Result<Self> {
            if text.is_empty() {
                bail!("password must not be empty");
            }
            let mut chars = Vec::with_capacity(text.chars().count());
            for ch in text.chars() {
                let scan = unsafe { VkKeyScanW(ch as u16) };
                if scan == -1 {
                    bail!(
                        "password character `{ch}` cannot be typed on the current keyboard layout"
                    );
                }
                let scan = scan as u16;
                let shift = (scan >> 8) as u8;
                if shift & !1 != 0 {
                    bail!("password character `{ch}` needs Ctrl/Alt; use letters or shift-only symbols");
                }
                chars.push(PasswordChar {
                    vk: VIRTUAL_KEY(scan & 0xFF),
                    shift: shift & 1 != 0,
                });
            }
            Ok(Password {
                chars,
                progress: 0,
                down: [false; 256],
            })
        }

        fn reset(&mut self) {
            self.progress = 0;
            self.down = [false; 256];
        }

        fn progress(&self) -> usize {
            self.progress
        }

        /// Feed one event. Returns true when the last character completed the
        /// password. `shift` is the tracked modifier state, not CapsLock.
        fn key(&mut self, vk: VIRTUAL_KEY, down: bool, shift: bool) -> bool {
            let idx = vk.0 as usize;
            if idx >= self.down.len() {
                return false;
            }
            if !down {
                self.down[idx] = false;
                return false;
            }
            if self.down[idx] {
                return false; // auto-repeat
            }
            self.down[idx] = true;
            if is_modifier(vk) {
                return false;
            }

            let expected = self.chars[self.progress];
            if expected.vk == vk && expected.shift == shift {
                self.progress += 1;
                if self.progress == self.chars.len() {
                    self.progress = 0;
                    return true;
                }
            } else if self.chars[0].vk == vk && self.chars[0].shift == shift {
                // A wrong key that happens to be the first character starts a
                // new attempt instead of forcing the user to begin again.
                self.progress = 1;
            } else {
                self.progress = 0;
            }
            false
        }
    }

    pub struct Hooks {
        keyboard: HHOOK,
        mouse: Option<HHOOK>,
    }

    impl Drop for Hooks {
        fn drop(&mut self) {
            unsafe {
                let _ = UnhookWindowsHookEx(self.keyboard);
                if let Some(mouse) = self.mouse {
                    let _ = UnhookWindowsHookEx(mouse);
                }
            }
        }
    }

    /// Install the low-level hooks. `combo` is the toggle; while locked the
    /// keyboard hook swallows everything except the combo/password, and the
    /// mouse hook swallows buttons but lets movement through.
    pub fn install(
        combo: kit::keys::Combo,
        password: Password,
        lock_mouse: bool,
        unlock_with_shortcut: bool,
    ) -> Result<Hooks> {
        COMBO.get_or_init(|| combo);
        UNLOCK_WITH_SHORTCUT.store(unlock_with_shortcut, Ordering::SeqCst);
        PASSWORD.with(|p| *p.borrow_mut() = Some(password));
        debug_log(format!(
            "install lock_mouse={lock_mouse} unlock_with_shortcut={unlock_with_shortcut}"
        ));

        let hmod = unsafe {
            windows::Win32::System::LibraryLoader::GetModuleHandleW(None)
                .unwrap_or_default()
                .into()
        };
        let keyboard = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), Some(hmod), 0) }
            .context("keyboard hook")?;
        let mouse = if lock_mouse {
            match unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), Some(hmod), 0) } {
                Ok(hook) => Some(hook),
                Err(e) => {
                    unsafe {
                        let _ = UnhookWindowsHookEx(keyboard);
                    }
                    return Err(e).context("mouse hook");
                }
            }
        } else {
            None
        };

        Ok(Hooks { keyboard, mouse })
    }

    static COMBO: std::sync::OnceLock<kit::keys::Combo> = std::sync::OnceLock::new();

    thread_local! {
        static MODS: std::cell::RefCell<Mods> = const {
            std::cell::RefCell::new(Mods {
                ctrl: false,
                shift: false,
                alt: false,
                win: false,
                ralt: false,
            })
        };
        static PASSWORD: std::cell::RefCell<Option<Password>> = const {
            std::cell::RefCell::new(None)
        };
    }

    unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 {
            let kb = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
            let down = matches!(wparam.0 as u32, 0x0100 | 0x0104); // WM_KEYDOWN | WM_SYSKEYDOWN
            let mut vk = VIRTUAL_KEY(kb.vkCode as u16);
            // An IME can report VK_PROCESSKEY for the physical key; recover the
            // real VK from the scan code so the password still types through.
            if vk == VK_PROCESSKEY {
                let mapped = unsafe { MapVirtualKeyW(kb.scanCode, MAPVK_VSC_TO_VK) };
                debug_log(format!("processkey scan={} mapped={mapped}", kb.scanCode));
                if mapped != 0 {
                    vk = VIRTUAL_KEY(mapped as u16);
                }
            }
            let was_locked = LOCKED.load(Ordering::SeqCst);

            let action = MODS.with(|m| {
                let combo = *COMBO.get().unwrap_or(&kit::keys::Combo::default());
                decide(&combo, &mut m.borrow_mut(), vk, down, was_locked)
            });

            match action {
                Action::Toggle => {
                    if was_locked && !UNLOCK_WITH_SHORTCUT.load(Ordering::SeqCst) {
                        // Password mode: the lock hotkey only locks. Swallow
                        // the chord so it cannot bypass the password.
                        return LRESULT(1);
                    }
                    let now = !was_locked;
                    LOCKED.store(now, Ordering::SeqCst);
                    PASSWORD_ARMED.store(false, Ordering::SeqCst);
                    debug_log(format!("toggle locked={now}"));
                    PASSWORD.with(|p| {
                        if let Some(p) = p.borrow_mut().as_mut() {
                            p.reset();
                        }
                    });
                    // Wake the tray loop to show the notification.
                    unsafe {
                        let _ = PostMessageW(None, WM_APP, WPARAM(usize::from(now)), LPARAM(0));
                    }
                    return LRESULT(1); // swallow the combo either way
                }
                Action::Swallow => {
                    if was_locked {
                        let ready = MODS.with(|m| !any_modifier(&m.borrow()));
                        if !PASSWORD_ARMED.load(Ordering::SeqCst) {
                            if !ready {
                                return LRESULT(1);
                            }
                            PASSWORD_ARMED.store(true, Ordering::SeqCst);
                        }
                        let shift = MODS.with(|m| m.borrow().shift);
                        let unlocked = PASSWORD.with(|p| {
                            p.borrow_mut()
                                .as_mut()
                                .is_some_and(|p| p.key(vk, down, shift))
                        });
                        let progress =
                            PASSWORD.with(|p| p.borrow().as_ref().map_or(0, Password::progress));
                        debug_log(format!(
                            "key vk={} down={down} shift={shift} progress={progress}",
                            vk.0
                        ));
                        if unlocked {
                            LOCKED.store(false, Ordering::SeqCst);
                            debug_log("password unlocked");
                            unsafe {
                                let _ = PostMessageW(None, WM_APP, WPARAM(0), LPARAM(0));
                            }
                        }
                    }
                    return LRESULT(1);
                }
                Action::Pass => {
                    // Only modifier releases pass while locked; feed them to
                    // the password state to clear its auto-repeat map.
                    if was_locked {
                        if MODS.with(|m| !any_modifier(&m.borrow())) {
                            PASSWORD_ARMED.store(true, Ordering::SeqCst);
                        }
                        let shift = MODS.with(|m| m.borrow().shift);
                        let _ = PASSWORD.with(|p| {
                            p.borrow_mut()
                                .as_mut()
                                .is_some_and(|p| p.key(vk, down, shift))
                        });
                    }
                }
            }
        }
        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }

    unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 {
            let msg = wparam.0 as u32;
            if is_blocked_button(msg) {
                let locked = LOCKED.load(Ordering::SeqCst);
                debug_log(format!("mouse msg={msg} locked={locked}"));
                if locked {
                    return LRESULT(1);
                }
            }
        }
        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }

    fn is_blocked_button(msg: u32) -> bool {
        matches!(
            msg,
            WM_LBUTTONDOWN
                | WM_LBUTTONUP
                | WM_RBUTTONDOWN
                | WM_RBUTTONUP
                | WM_MBUTTONDOWN
                | WM_MBUTTONUP
                | WM_LBUTTONDBLCLK
                | WM_RBUTTONDBLCLK
                | WM_MBUTTONDBLCLK
        )
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn combo(s: &str) -> kit::keys::Combo {
            s.parse().unwrap()
        }

        fn letter(ch: char) -> VIRTUAL_KEY {
            VIRTUAL_KEY(ch.to_ascii_uppercase() as u16)
        }

        fn simple_password() -> Password {
            let chars = "unlock"
                .chars()
                .map(|ch| PasswordChar {
                    vk: letter(ch),
                    shift: false,
                })
                .collect();
            Password {
                chars,
                progress: 0,
                down: [false; 256],
            }
        }

        fn press(p: &mut Password, vk: VIRTUAL_KEY, shift: bool) -> bool {
            let done = p.key(vk, true, shift);
            p.key(vk, false, shift);
            done
        }

        #[test]
        fn exact_combo_toggles() {
            let c = combo("ctrl+alt+shift+insert");
            let mut m = Mods::default();
            assert_eq!(decide(&c, &mut m, VK_LCONTROL, true, false), Action::Pass);
            assert_eq!(decide(&c, &mut m, VK_LMENU, true, false), Action::Pass);
            assert_eq!(decide(&c, &mut m, VK_LSHIFT, true, false), Action::Pass);
            assert_eq!(decide(&c, &mut m, VK_INSERT, true, false), Action::Toggle);
            assert_eq!(decide(&c, &mut m, VK_INSERT, false, false), Action::Pass);
        }

        #[test]
        fn extra_modifier_does_not_toggle() {
            let c = combo("alt+x");
            let mut m = Mods::default();
            decide(&c, &mut m, VK_LCONTROL, true, false);
            decide(&c, &mut m, VK_LMENU, true, false);
            assert_eq!(decide(&c, &mut m, VK_X, true, false), Action::Pass);
        }

        #[test]
        fn locked_swallows_other_keys() {
            let c = combo("ctrl+alt+shift+insert");
            let mut m = Mods::default();
            assert_eq!(decide(&c, &mut m, VK_X, true, true), Action::Swallow);
            assert_eq!(decide(&c, &mut m, VK_X, false, true), Action::Swallow);
            assert_eq!(decide(&c, &mut m, VK_X, false, false), Action::Pass);
            // Modifier releases pass through so they cannot get stuck.
            assert_eq!(decide(&c, &mut m, VK_LCONTROL, false, true), Action::Pass);
        }

        #[test]
        fn win_and_altgr_combos_match() {
            let win = combo("win+x");
            let mut m = Mods::default();
            decide(&win, &mut m, VK_LWIN, true, false);
            assert_eq!(decide(&win, &mut m, VK_X, true, false), Action::Toggle);

            let altgr = combo("altgr+x");
            let mut m = Mods::default();
            decide(&altgr, &mut m, VK_LCONTROL, true, false);
            decide(&altgr, &mut m, VK_RMENU, true, false);
            assert_eq!(decide(&altgr, &mut m, VK_X, true, false), Action::Toggle);

            let plain = combo("ctrl+alt+x");
            let mut m = Mods::default();
            decide(&plain, &mut m, VK_LCONTROL, true, false);
            decide(&plain, &mut m, VK_RMENU, true, false);
            assert_eq!(decide(&plain, &mut m, VK_X, true, false), Action::Pass);
        }

        #[test]
        fn password_unlocks_on_the_last_character() {
            let mut p = simple_password();
            for ch in "unloc".chars() {
                assert!(!press(&mut p, letter(ch), false));
            }
            assert!(press(&mut p, letter('k'), false));
        }

        #[test]
        fn password_resets_on_a_wrong_character() {
            let mut p = simple_password();
            press(&mut p, VK_U, false);
            press(&mut p, VK_X, false);
            assert_eq!(p.progress, 0);
            for ch in "unloc".chars() {
                assert!(!press(&mut p, letter(ch), false));
            }
            assert!(press(&mut p, letter('k'), false));
        }

        #[test]
        fn password_requires_shift_for_shifted_characters() {
            let mut p = Password {
                chars: vec![PasswordChar {
                    vk: VK_U,
                    shift: true,
                }],
                progress: 0,
                down: [false; 256],
            };
            assert!(!press(&mut p, VK_U, false));
            assert!(press(&mut p, VK_U, true));
        }

        #[test]
        fn password_ignores_auto_repeat() {
            let mut p = simple_password();
            assert!(!p.key(VK_U, true, false));
            assert!(!p.key(VK_U, true, false)); // repeat while held
            assert_eq!(p.progress, 1);
            p.key(VK_U, false, false);
            for ch in "nloc".chars() {
                assert!(!press(&mut p, letter(ch), false));
            }
            assert!(press(&mut p, letter('k'), false));
        }

        #[test]
        fn default_password_maps_six_characters() {
            let p = Password::new("unlock").unwrap();
            assert_eq!(p.chars.len(), 6);
            assert!(Password::new("").is_err());
        }

        #[test]
        fn mouse_buttons_are_blocked_but_motion_and_wheel_pass() {
            assert!(is_blocked_button(WM_LBUTTONDOWN));
            assert!(is_blocked_button(WM_RBUTTONUP));
            assert!(is_blocked_button(WM_MBUTTONDOWN));
            assert!(!is_blocked_button(WM_MOUSEMOVE));
            assert!(!is_blocked_button(WM_MOUSEWHEEL));
        }
    }
}

#[cfg(all(test, windows))]
mod notify_tests {
    use super::*;

    #[test]
    fn modes_parse() {
        assert!(matches!(
            parse_notify_mode(None, None).unwrap(),
            NotifyMode::System
        ));
        assert!(matches!(
            parse_notify_mode(Some("system"), None).unwrap(),
            NotifyMode::System
        ));
        assert!(matches!(
            parse_notify_mode(Some("SILENT"), None).unwrap(),
            NotifyMode::Silent
        ));
        assert!(matches!(
            parse_notify_mode(Some("off"), None).unwrap(),
            NotifyMode::Silent
        ));
        assert!(matches!(
            parse_notify_mode(
                Some("command"),
                Some("snoretoast -t \"{title}\" -m \"{body}\"")
            )
            .unwrap(),
            NotifyMode::Command(_)
        ));
        assert!(parse_notify_mode(Some("command"), None).is_err());
        assert!(parse_notify_mode(Some("bogus"), None).is_err());
    }

    #[test]
    fn command_placeholders_are_replaced() {
        assert_eq!(
            render_command("snoretoast -t \"{title}\" -m \"{body}\"", "T", "B"),
            "snoretoast -t \"T\" -m \"B\""
        );
    }
}

#[cfg(windows)]
#[derive(serde::Deserialize, Default)]
struct NotifySettings {
    mode: Option<String>,
    command: Option<String>,
}

#[cfg(windows)]
enum NotifyMode {
    System,
    Command(String),
    Silent,
}

#[cfg(windows)]
fn parse_notify_mode(mode: Option<&str>, command: Option<&str>) -> anyhow::Result<NotifyMode> {
    match mode
        .unwrap_or("system")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "system" => Ok(NotifyMode::System),
        "silent" | "off" | "none" => Ok(NotifyMode::Silent),
        "command" | "custom" => {
            let command = command.unwrap_or("").trim();
            if command.is_empty() {
                anyhow::bail!("notify.command is required when notify.mode = \"command\"");
            }
            Ok(NotifyMode::Command(command.to_string()))
        }
        other => anyhow::bail!("unknown notify.mode `{other}` (system, command, silent)"),
    }
}

#[cfg(windows)]
fn render_command(template: &str, title: &str, body: &str) -> String {
    template.replace("{title}", title).replace("{body}", body)
}

#[cfg(windows)]
impl NotifyMode {
    fn send(
        &self,
        tray: Option<&kit::tray::Tray>,
        link: Option<&kit::ipc::PluginLink>,
        title: &str,
        body: &str,
    ) {
        match self {
            NotifyMode::System => {
                let result = if let Some(link) = link {
                    link.notify(title, body)
                } else if let Some(tray) = tray {
                    tray.notify(title, body)
                } else {
                    return;
                };
                match result {
                    Ok(()) => lock::debug_log("notify system ok"),
                    Err(e) => {
                        lock::debug_log(format!("notify system failed: {e:#}"));
                        eprintln!("notify: {e:#}");
                    }
                }
            }
            NotifyMode::Command(template) => {
                use std::os::windows::process::CommandExt;
                const CREATE_NO_WINDOW: u32 = 0x0800_0000;
                let _ = std::process::Command::new("cmd")
                    .args(["/C", &render_command(template, title, body)])
                    .creation_flags(CREATE_NO_WINDOW)
                    .spawn();
            }
            NotifyMode::Silent => {}
        }
    }
}

#[cfg(windows)]
fn run() -> anyhow::Result<()> {
    use kit::keys::Combo;
    use kit::tray::{Event, Tray};
    use kit::{config, win};
    use serde::Deserialize;

    const APP: &str = "keyboard-locker";
    let hosted = std::env::args().any(|a| a == "--plugin");
    let no_tray = hosted && std::env::args().any(|a| a == "--no-tray");

    let _single = win::ensure_single_instance(APP)?;

    #[derive(Deserialize, Default)]
    struct General {
        shortcut: Option<String>,
        password: Option<String>,
        lock_mouse: Option<bool>,
        unlock_with_shortcut: Option<bool>,
    }
    #[derive(Deserialize, Default)]
    struct FileConfig {
        general: Option<General>,
        notify: Option<NotifySettings>,
    }

    let (config_path, text) = config::load_profile(
        "keyboard-locker.toml",
        include_str!("../keyboard-locker.toml"),
    )?;
    let file: FileConfig = toml::from_str(&text)?;
    let notify = file.notify.unwrap_or_default();
    let notify_mode = parse_notify_mode(notify.mode.as_deref(), notify.command.as_deref())?;
    let general = file.general.unwrap_or_default();
    let combo: Combo = general
        .shortcut
        .unwrap_or_else(|| "ctrl+alt+shift+insert".into())
        .parse()?;
    let password = lock::Password::new(general.password.as_deref().unwrap_or("unlock"))?;
    let lock_mouse = general.lock_mouse.unwrap_or(true);
    let unlock_with_shortcut = general.unlock_with_shortcut.unwrap_or(false);

    let _hooks = lock::install(combo, password, lock_mouse, unlock_with_shortcut)?;

    if no_tray {
        use kit::ipc::HostCommand;
        let link = kit::ipc::PluginLink::new()?;
        link.ready(kit::menu::MenuModel::default(), Some("unlocked".into()))?;
        kit::tray::run_headless(None, |event| match event {
            Event::Wakeup(4) => {
                while let Some(command) = link.try_recv() {
                    if matches!(command, HostCommand::Quit { .. }) {
                        kit::tray::quit();
                    }
                }
            }
            Event::Wakeup(state) => {
                let locked = state == 1;
                let _ = link.status(if locked { "locked" } else { "unlocked" });
                let (title, body) = if locked {
                    (
                        "keyboard-locker",
                        "Keyboard locked. Type your password to unlock.",
                    )
                } else {
                    ("keyboard-locker", "Keyboard unlocked.")
                };
                notify_mode.send(None, Some(&link), title, body);
            }
            _ => {}
        });
        return Ok(());
    }

    // Standalone mode keeps a local tray for config/reload/exit.
    let mut tray = Tray::new(APP, icon())?;
    refresh_menu(&mut tray, hosted);

    tray.run(None, |tray, event| match event {
        Event::Wakeup(state) => {
            let (title, body) = if state == 1 {
                (
                    "keyboard-locker",
                    "Keyboard locked. Type your password to unlock.",
                )
            } else {
                ("keyboard-locker", "Keyboard unlocked.")
            };
            notify_mode.send(Some(tray), None, title, body);
        }
        Event::Menu(label) if label == "Start with Windows" => {
            let now = !win::autostart_enabled(APP);
            if let Err(e) = win::set_autostart(APP, now) {
                win::message_box(APP, &format!("Autostart failed:\n{e:#}"));
            }
            refresh_menu(tray, hosted);
        }
        Event::Menu(label) if label == "Edit Config" => {
            win::edit_config(&config_path);
        }
        Event::Menu(label) if label == "Reload" => {
            config::restart_self();
            tray.quit();
        }
        Event::Menu(label) if label == "Exit" => tray.quit(),
        _ => {}
    });
    Ok(())
}

#[cfg(windows)]
fn icon() -> Vec<u8> {
    kit::icon::letter_rgba('l', [0x2e, 0x7d, 0x32, 0xff], [0x1b, 0x4d, 0x1d, 0xff])
}

#[cfg(windows)]
fn refresh_menu(tray: &mut kit::tray::Tray, hosted: bool) {
    use kit::tray::Entry;
    let mut entries = Vec::new();
    if !hosted {
        entries.push(Entry::Toggle(
            "Start with Windows",
            kit::win::autostart_enabled("keyboard-locker"),
        ));
        entries.push(Entry::Sep);
    }
    entries.push(Entry::Item("Edit Config"));
    if !hosted {
        entries.push(Entry::Item("Reload"));
        entries.push(Entry::Item("Exit"));
    }
    tray.set_menu(&entries);
}
