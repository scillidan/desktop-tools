//! TOML config: flat `combo = "action"` table plus a few scalar settings
//! (`gap`, `dragModifier`), first-run create next to the exe (keymap-rs /
//! keytools-rs convention).

use crate::keys::{Combo, Mods};
use crate::layout::SizeSpec;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::SystemTime;
use window_app_switch::config::{AppSwitchAction, AppSwitchConfig};
use window_app_toggle::config::Config as AppToggleConfig;
use window_maximize_minimize::config::MaximizeMinimizeConfig;
use window_mouse_drag::config::MouseDragConfig;
use window_on_top_bottom::config::OnTopBottomConfig;
use window_quick_switch::config::{QuickSwitchAction, QuickSwitchConfig};
use window_rectangle::config::RectangleConfig;
use window_resize::config::ResizeConfig;
use window_virtual_desktops::config::{VirtualDesktopAction, VirtualDesktopConfig};

/// Everything a hotkey can do. Names match RectangleWin(Plus) actions.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    MoveToLeft,
    MoveToRight,
    MoveToTop,
    MoveToBottom,
    MoveToTopLeft,
    MoveToTopRight,
    MoveToBottomLeft,
    MoveToBottomRight,
    MoveToCenter,
    Maximize,
    NextDisplay,
    PrevDisplay,
    SwapWindows,
    ToggleAlwaysOnTop,
    ToggleAlwaysOnBottom,
    Minimize,
    UnpinAll,
    Scale,
    ToggleWezTerm,
    ToggleDoubleCommander,
    NextWindow,
    PrevWindow,
    SizerMenu,
    Undo,
    Quit,
    /// Put the file dialog in front into the last file manager's folder.
    FileDialog,
    // Imported by keyboard-shell; Window does not handle this action.
    Suppress,
    /// Switch to the Nth virtual desktop (1-9).
    SwitchToDesktop(u8),
    /// Move the foreground window to the Nth virtual desktop (1-9).
    MoveToDesktop(u8),
    /// Switch to the next/prev virtual desktop, wrapping around.
    NextDesktop,
    PrevDesktop,
}

impl FromStr for Action {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Action> {
        Ok(match s {
            "moveToLeft" => Action::MoveToLeft,
            "moveToRight" => Action::MoveToRight,
            "moveToTop" => Action::MoveToTop,
            "moveToBottom" => Action::MoveToBottom,
            "moveToTopLeft" => Action::MoveToTopLeft,
            "moveToTopRight" => Action::MoveToTopRight,
            "moveToBottomLeft" => Action::MoveToBottomLeft,
            "moveToBottomRight" => Action::MoveToBottomRight,
            "moveToCenter" => Action::MoveToCenter,
            "maximize" => Action::Maximize,
            "nextDisplay" => Action::NextDisplay,
            "prevDisplay" => Action::PrevDisplay,
            "swapWindows" => Action::SwapWindows,
            "toggleAlwaysOnTop" => Action::ToggleAlwaysOnTop,
            "toggleAlwaysOnBottom" => Action::ToggleAlwaysOnBottom,
            "minimize" => Action::Minimize,
            "unpinAll" => Action::UnpinAll,
            "scale" => Action::Scale,
            "toggleWezTerm" => Action::ToggleWezTerm,
            "toggleDoubleCommander" => Action::ToggleDoubleCommander,
            "nextWindow" => Action::NextWindow,
            "prevWindow" => Action::PrevWindow,
            "sizerMenu" => Action::SizerMenu,
            "undo" => Action::Undo,
            "fileDialog" => Action::FileDialog,
            "suppress" => Action::Suppress,
            "nextDesktop" => Action::NextDesktop,
            "prevDesktop" => Action::PrevDesktop,
            "quit" => Action::Quit,
            other => {
                if let Some(n) = numbered(other, "switchToDesktop") {
                    return Ok(Action::SwitchToDesktop(n));
                }
                if let Some(n) = numbered(other, "moveToDesktop") {
                    return Ok(Action::MoveToDesktop(n));
                }
                anyhow::bail!(
                    "unknown action `{other}` (known: {})",
                    Action::names().join(", ")
                )
            }
        })
    }
}

/// `switchToDesktop3` -> Some(3); only 1..=9, so a typo stays an error.
fn numbered(s: &str, prefix: &str) -> Option<u8> {
    let n = s.strip_prefix(prefix)?.parse::<u8>().ok()?;
    (1..=9).contains(&n).then_some(n)
}

impl Action {
    pub fn names() -> &'static [&'static str] {
        &[
            "moveToLeft",
            "moveToRight",
            "moveToTop",
            "moveToBottom",
            "moveToTopLeft",
            "moveToTopRight",
            "moveToBottomLeft",
            "moveToBottomRight",
            "moveToCenter",
            "maximize",
            "nextDisplay",
            "prevDisplay",
            "swapWindows",
            "toggleAlwaysOnTop",
            "toggleAlwaysOnBottom",
            "minimize",
            "unpinAll",
            "scale",
            "toggleWezTerm",
            "toggleDoubleCommander",
            "nextWindow",
            "prevWindow",
            "sizerMenu",
            "undo",
            "fileDialog",
            "suppress",
            "nextDesktop",
            "prevDesktop",
            "quit",
            "switchToDesktop1..9",
            "moveToDesktop1..9",
        ]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    /// Pixels of outer margin between a snapped window and the work area.
    pub gap: i32,
    /// Modifiers held with the left mouse button to drag-move a window.
    pub drag_modifier: Mods,
    /// Put file dialogs into Explorer's current folder (Windows).
    pub quick_switch: bool,
    /// Milliseconds between always-on-bottom re-asserts (Windows).
    pub bottom_reassert_ms: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            gap: 0,
            // A bare Win tap opens the Start menu; Win+Alt is a safe default
            // and still one-handed.
            drag_modifier: Mods {
                ctrl: false,
                shift: false,
                alt: true,
                win: true,
            },
            quick_switch: true,
            bottom_reassert_ms: 250,
        }
    }
}

pub type Binding = window_core::Binding<Action>;

pub struct Config {
    pub bindings: Vec<Binding>,
    pub settings: Settings,
    pub modules: Modules,
    pub global: Vec<Binding>,
}

pub struct Modules {
    pub rectangle: RectangleConfig,
    pub resize: ResizeConfig,
    pub scale: ResizeConfig,
    pub mouse_drag: MouseDragConfig,
    pub on_top_bottom: OnTopBottomConfig,
    pub maximize_minimize: MaximizeMinimizeConfig,
    pub app_switch: AppSwitchConfig,
    pub app_toggle: AppToggleConfig,
    pub quick_switch: QuickSwitchConfig,
    pub virtual_desktops: VirtualDesktopConfig,
}

fn legacy_app_toggle(bindings: &[Binding]) -> AppToggleConfig {
    AppToggleConfig {
        bindings: bindings
            .iter()
            .filter_map(|b| {
                let action = match b.action {
                    Action::ToggleWezTerm => window_app_toggle::AppToggleAction::WezTerm,
                    Action::ToggleDoubleCommander => {
                        window_app_toggle::AppToggleAction::DoubleCommander
                    }
                    _ => return None,
                };
                Some(window_core::Binding {
                    combo: b.combo,
                    action,
                })
            })
            .collect(),
        ..AppToggleConfig::default()
    }
}

fn legacy_modules(bindings: &[Binding], settings: Settings) -> Modules {
    Modules {
        rectangle: RectangleConfig {
            bindings: bindings
                .iter()
                .filter_map(|b| {
                    use window_rectangle::RectangleAction as A;
                    let action = match b.action {
                        Action::MoveToLeft => A::MoveToLeft,
                        Action::MoveToRight => A::MoveToRight,
                        Action::MoveToTop => A::MoveToTop,
                        Action::MoveToBottom => A::MoveToBottom,
                        Action::MoveToTopLeft => A::MoveToTopLeft,
                        Action::MoveToTopRight => A::MoveToTopRight,
                        Action::MoveToBottomLeft => A::MoveToBottomLeft,
                        Action::MoveToBottomRight => A::MoveToBottomRight,
                        Action::MoveToCenter => A::MoveToCenter,
                        Action::NextDisplay => A::NextDisplay,
                        Action::PrevDisplay => A::PrevDisplay,
                        Action::SwapWindows => A::SwapWindows,
                        _ => return None,
                    };
                    Some(window_core::Binding {
                        combo: b.combo,
                        action,
                    })
                })
                .collect(),
            settings: window_rectangle::config::RectangleSettings { gap: settings.gap },
        },
        resize: ResizeConfig {
            bindings: bindings
                .iter()
                .filter(|b| b.action == Action::SizerMenu)
                .map(|b| window_core::Binding {
                    combo: b.combo,
                    action: window_resize::ResizeAction::SizerMenu,
                })
                .collect(),
        },
        scale: ResizeConfig {
            bindings: bindings
                .iter()
                .filter(|b| b.action == Action::Scale)
                .map(|b| window_core::Binding {
                    combo: b.combo,
                    action: window_resize::ResizeAction::Scale,
                })
                .collect(),
        },
        mouse_drag: MouseDragConfig {
            settings: window_mouse_drag::config::MouseDragSettings {
                drag_modifier: settings.drag_modifier,
            },
        },
        on_top_bottom: OnTopBottomConfig {
            bindings: bindings
                .iter()
                .filter_map(|b| {
                    use window_on_top_bottom::OnTopBottomAction as A;
                    let action = match b.action {
                        Action::ToggleAlwaysOnTop => A::ToggleAlwaysOnTop,
                        Action::ToggleAlwaysOnBottom => A::ToggleAlwaysOnBottom,
                        Action::UnpinAll => A::ClearAllPins,
                        _ => return None,
                    };
                    Some(window_core::Binding {
                        combo: b.combo,
                        action,
                    })
                })
                .collect(),
            settings: window_on_top_bottom::config::OnTopBottomSettings {
                bottom_reassert_ms: settings.bottom_reassert_ms,
            },
        },
        maximize_minimize: MaximizeMinimizeConfig {
            bindings: bindings
                .iter()
                .filter_map(|b| {
                    use window_maximize_minimize::MaximizeMinimizeAction as A;
                    let action = match b.action {
                        Action::Maximize => A::Maximize,
                        Action::Minimize => A::Minimize,
                        _ => return None,
                    };
                    Some(window_core::Binding {
                        combo: b.combo,
                        action,
                    })
                })
                .collect(),
        },
        app_switch: AppSwitchConfig {
            bindings: bindings
                .iter()
                .filter_map(|b| {
                    let action = match b.action {
                        Action::NextWindow => AppSwitchAction::NextWindow,
                        Action::PrevWindow => AppSwitchAction::PrevWindow,
                        _ => return None,
                    };
                    Some(window_core::Binding {
                        combo: b.combo,
                        action,
                    })
                })
                .collect(),
        },
        app_toggle: legacy_app_toggle(bindings),
        quick_switch: QuickSwitchConfig {
            bindings: bindings
                .iter()
                .filter(|b| b.action == Action::FileDialog)
                .map(|b| window_core::Binding {
                    combo: b.combo,
                    action: QuickSwitchAction::FileDialog,
                })
                .collect(),
            settings: window_quick_switch::config::QuickSwitchSettings {
                enabled: settings.quick_switch,
            },
        },
        virtual_desktops: VirtualDesktopConfig {
            bindings: bindings
                .iter()
                .filter_map(|b| {
                    let action = match b.action {
                        Action::NextDesktop => VirtualDesktopAction::NextDesktop,
                        Action::PrevDesktop => VirtualDesktopAction::PrevDesktop,
                        Action::SwitchToDesktop(n) => VirtualDesktopAction::SwitchToDesktop(n),
                        Action::MoveToDesktop(n) => VirtualDesktopAction::MoveToDesktop(n),
                        _ => return None,
                    };
                    Some(window_core::Binding {
                        combo: b.combo,
                        action,
                    })
                })
                .collect(),
        },
    }
}

fn assemble(modules: Modules, global: Vec<Binding>) -> Result<Config> {
    let mut bindings = global.clone();
    bindings.extend(modules.rectangle.bindings.iter().map(|b| Binding {
        combo: b.combo,
        action: match b.action {
            window_rectangle::RectangleAction::MoveToLeft => Action::MoveToLeft,
            window_rectangle::RectangleAction::MoveToRight => Action::MoveToRight,
            window_rectangle::RectangleAction::MoveToTop => Action::MoveToTop,
            window_rectangle::RectangleAction::MoveToBottom => Action::MoveToBottom,
            window_rectangle::RectangleAction::MoveToTopLeft => Action::MoveToTopLeft,
            window_rectangle::RectangleAction::MoveToTopRight => Action::MoveToTopRight,
            window_rectangle::RectangleAction::MoveToBottomLeft => Action::MoveToBottomLeft,
            window_rectangle::RectangleAction::MoveToBottomRight => Action::MoveToBottomRight,
            window_rectangle::RectangleAction::MoveToCenter => Action::MoveToCenter,
            window_rectangle::RectangleAction::NextDisplay => Action::NextDisplay,
            window_rectangle::RectangleAction::PrevDisplay => Action::PrevDisplay,
            window_rectangle::RectangleAction::SwapWindows => Action::SwapWindows,
        },
    }));
    bindings.extend(modules.resize.bindings.iter().map(|b| Binding {
        combo: b.combo,
        action: match b.action {
            window_resize::ResizeAction::Scale => Action::Scale,
            window_resize::ResizeAction::SizerMenu => Action::SizerMenu,
        },
    }));
    bindings.extend(modules.scale.bindings.iter().map(|b| Binding {
        combo: b.combo,
        action: match b.action {
            window_resize::ResizeAction::Scale => Action::Scale,
            window_resize::ResizeAction::SizerMenu => Action::SizerMenu,
        },
    }));
    bindings.extend(modules.on_top_bottom.bindings.iter().map(|b| Binding {
        combo: b.combo,
        action: match b.action {
            window_on_top_bottom::OnTopBottomAction::ToggleAlwaysOnTop => Action::ToggleAlwaysOnTop,
            window_on_top_bottom::OnTopBottomAction::ToggleAlwaysOnBottom => {
                Action::ToggleAlwaysOnBottom
            }
            window_on_top_bottom::OnTopBottomAction::ClearAllPins => Action::UnpinAll,
        },
    }));
    bindings.extend(modules.maximize_minimize.bindings.iter().map(|b| Binding {
        combo: b.combo,
        action: match b.action {
            window_maximize_minimize::MaximizeMinimizeAction::Maximize => Action::Maximize,
            window_maximize_minimize::MaximizeMinimizeAction::Minimize => Action::Minimize,
        },
    }));
    bindings.extend(modules.app_switch.bindings.iter().map(|b| Binding {
        combo: b.combo,
        action: match b.action {
            AppSwitchAction::NextWindow => Action::NextWindow,
            AppSwitchAction::PrevWindow => Action::PrevWindow,
        },
    }));
    bindings.extend(modules.app_toggle.bindings.iter().map(|b| Binding {
        combo: b.combo,
        action: match b.action {
            window_app_toggle::AppKind::WezTerm => Action::ToggleWezTerm,
            window_app_toggle::AppKind::DoubleCommander => Action::ToggleDoubleCommander,
        },
    }));
    bindings.extend(modules.quick_switch.bindings.iter().map(|b| Binding {
        combo: b.combo,
        action: Action::FileDialog,
    }));
    bindings.extend(modules.virtual_desktops.bindings.iter().map(|b| Binding {
        combo: b.combo,
        action: match b.action {
            VirtualDesktopAction::NextDesktop => Action::NextDesktop,
            VirtualDesktopAction::PrevDesktop => Action::PrevDesktop,
            VirtualDesktopAction::SwitchToDesktop(n) => Action::SwitchToDesktop(n),
            VirtualDesktopAction::MoveToDesktop(n) => Action::MoveToDesktop(n),
        },
    }));
    bindings.sort_by_key(|b| b.combo);
    for pair in bindings.windows(2) {
        if pair[0].combo == pair[1].combo {
            anyhow::bail!(
                "hotkey `{}` is assigned by more than one module",
                pair[0].combo
            );
        }
    }
    let settings = Settings {
        gap: modules.rectangle.settings.gap,
        drag_modifier: modules.mouse_drag.settings.drag_modifier,
        quick_switch: modules.quick_switch.settings.enabled,
        bottom_reassert_ms: modules.on_top_bottom.settings.bottom_reassert_ms,
    };
    Ok(Config {
        bindings,
        settings,
        modules,
        global,
    })
}

/// Sizer presets live next to `window-actions.toml`, in their own file.
pub fn load_or_create_presets(path: &Path, default_toml: &str) -> Result<Vec<SizeSpec>> {
    let exe = std::env::current_exe().context("cannot locate own executable")?;
    let legacy = exe.parent().context("executable has no parent directory")?;
    kit::config::import_legacy_once(path, legacy, &["resize.toml"])?;
    kit::config::write_reference_in_profile(
        path.parent().context("config path has no parent")?,
        "resize.toml",
        default_toml,
    )?;
    if !path.exists() {
        std::fs::write(
            path,
            kit::config::user_hotkey_patch_toml(default_toml, default_toml)?,
        )
        .with_context(|| format!("cannot create {}", path.display()))?;
    }
    let user =
        std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    parse_presets(&kit::config::merge_hotkey_toml(default_toml, &user)?)
}

/// Re-read `resize.toml`; the daemon calls this on each Sizer hotkey.
pub fn load_presets(path: &Path) -> Result<Vec<SizeSpec>> {
    let user =
        std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    let text = kit::config::merge_hotkey_toml(include_str!("../resize.toml"), &user)?;
    parse_presets(&text)
}

/// Presets are kept in file order: the menu order is the user's order.
/// Bad entries are skipped, like bad bindings in `window-actions.toml`.
pub fn parse_presets(text: &str) -> Result<Vec<SizeSpec>> {
    let value: toml::Value = text.parse().context("resize.toml is not valid TOML")?;
    let table = value
        .as_table()
        .context("resize.toml root must be a TOML table")?;
    let items = table
        .get("presets")
        .and_then(|v| v.as_array())
        .context("resize.toml needs a `presets = [...]` array")?;
    let mut out = Vec::new();
    for item in items {
        match item.as_str() {
            Some(s) => match s.parse::<SizeSpec>() {
                Ok(spec) => out.push(spec),
                Err(e) => eprintln!("warn: skipping preset `{s}`: {e:#}"),
            },
            None => eprintln!("warn: skipping preset: expected a string"),
        }
    }
    Ok(out)
}

/// `<profile>/window-actions.toml`; feature bindings live in their own files.
pub const CONFIG_NAME: &str = "window-actions.toml";
pub const LEGACY_CONFIG_NAME: &str = "window-tools.toml";
pub const DEFAULT_CONFIG: &str = include_str!("../window-actions.toml");
#[cfg(test)]
pub const LEGACY_DEFAULT_CONFIG: &str = include_str!("../window-tools.legacy.toml");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigModule {
    Rectangle,
    Resize,
    Scale,
    MouseDrag,
    OnTopBottom,
    MaximizeMinimize,
    AppSwitch,
    AppToggle,
    QuickSwitch,
    VirtualDesktops,
}

pub const MODULES: [ConfigModule; 10] = [
    ConfigModule::Rectangle,
    ConfigModule::Resize,
    ConfigModule::Scale,
    ConfigModule::MouseDrag,
    ConfigModule::OnTopBottom,
    ConfigModule::MaximizeMinimize,
    ConfigModule::AppSwitch,
    ConfigModule::AppToggle,
    ConfigModule::QuickSwitch,
    ConfigModule::VirtualDesktops,
];

impl ConfigModule {
    pub fn file_name(self) -> &'static str {
        match self {
            Self::Rectangle => "window-rectangle.toml",
            Self::Resize => "window-resize.toml",
            Self::Scale => "window-scale.toml",
            Self::MouseDrag => "window-mouse-drag.toml",
            Self::OnTopBottom => "window-on-top-bottom.toml",
            Self::MaximizeMinimize => "window-maximize-minimize.toml",
            Self::AppSwitch => "window-app-switch.toml",
            Self::AppToggle => "window-app-toggle.toml",
            Self::QuickSwitch => "window-quick-switch.toml",
            Self::VirtualDesktops => "window-virtual-desktops.toml",
        }
    }

    pub fn path(self, global: &Path) -> Result<PathBuf> {
        Ok(global
            .parent()
            .context("config path has no parent")?
            .join(self.file_name()))
    }
}
// desktop-tools.toml is not a Window binding file.
const OLD_CONFIG_NAMES: &[&str] = &["window-tools.toml", "windowtools.toml", "rectangle.toml"];

pub fn default_config_path() -> Result<PathBuf> {
    let path = kit::config::profile_file(CONFIG_NAME)?;
    kit::config::write_reference_in_profile(
        path.parent().context("config path has no parent")?,
        CONFIG_NAME,
        DEFAULT_CONFIG,
    )?;
    let legacy_path = path.with_file_name(LEGACY_CONFIG_NAME);
    if !path.exists() && !legacy_path.exists() {
        let exe = std::env::current_exe().context("cannot locate own executable")?;
        let legacy = exe.parent().context("executable has no parent directory")?;
        kit::config::import_legacy_once(&legacy_path, legacy, OLD_CONFIG_NAMES)?;
    }
    Ok(path)
}

fn module_file<T>(global: &Path, module: ConfigModule, parse: fn(&str) -> Result<T>) -> Result<T> {
    let path = module.path(global)?;
    let user = std::fs::read_to_string(&path)
        .with_context(|| format!("cannot read {}", path.display()))?;
    let text = kit::config::merge_hotkey_toml(default_module(module), &user)
        .with_context(|| format!("cannot merge {}", path.display()))?;
    parse(&text).with_context(|| format!("invalid {}", path.display()))
}

fn load_modules(path: &Path, global: Vec<Binding>) -> Result<Config> {
    let modules = Modules {
        rectangle: module_file(
            path,
            ConfigModule::Rectangle,
            window_rectangle::config::parse,
        )?,
        resize: module_file(path, ConfigModule::Resize, window_resize::config::parse)?,
        scale: module_file(path, ConfigModule::Scale, window_resize::config::parse)?,
        mouse_drag: module_file(
            path,
            ConfigModule::MouseDrag,
            window_mouse_drag::config::parse,
        )?,
        on_top_bottom: module_file(
            path,
            ConfigModule::OnTopBottom,
            window_on_top_bottom::config::parse,
        )?,
        maximize_minimize: module_file(
            path,
            ConfigModule::MaximizeMinimize,
            window_maximize_minimize::config::parse,
        )?,
        app_switch: module_file(
            path,
            ConfigModule::AppSwitch,
            window_app_switch::config::parse,
        )?,
        app_toggle: module_file(
            path,
            ConfigModule::AppToggle,
            window_app_toggle::config::parse,
        )?,
        quick_switch: module_file(
            path,
            ConfigModule::QuickSwitch,
            window_quick_switch::config::parse,
        )?,
        virtual_desktops: module_file(
            path,
            ConfigModule::VirtualDesktops,
            window_virtual_desktops::config::parse,
        )?,
    };
    assemble(modules, global)
}

fn patch_with_removed_bindings(default: &str, effective: &str) -> Result<String> {
    fn matching_key(table: &toml::Table, key: &str) -> Option<String> {
        table
            .keys()
            .find(|existing| {
                existing.as_str() == key
                    || matches!(
                        (existing.parse::<Combo>(), key.parse::<Combo>()),
                        (Ok(existing), Ok(default)) if existing == default
                    )
            })
            .cloned()
    }

    fn mark_removed(defaults: &toml::Table, effective: &mut toml::Table) {
        for (key, default_value) in defaults {
            let existing = matching_key(effective, key);
            match existing {
                Some(existing) => {
                    if let (Some(default_table), Some(effective_table)) = (
                        default_value.as_table(),
                        effective
                            .get_mut(&existing)
                            .and_then(toml::Value::as_table_mut),
                    ) {
                        mark_removed(default_table, effective_table);
                    }
                }
                None if key.parse::<Combo>().is_ok() => {
                    effective.insert(key.clone(), toml::Value::Boolean(false));
                }
                None if default_value.is_array() => {
                    effective.insert(key.clone(), toml::Value::Array(Vec::new()));
                }
                None => {}
            }
        }
    }

    let defaults: toml::Value = toml::from_str(default)?;
    let mut effective: toml::Value = toml::from_str(effective)?;
    if let (Some(defaults), Some(effective)) = (defaults.as_table(), effective.as_table_mut()) {
        mark_removed(defaults, effective);
    }
    kit::config::user_hotkey_patch_toml(default, &toml::to_string_pretty(&effective)?)
}

/// Materialize only the legacy differences; the old file is never rewritten.
pub fn ensure_module_config(
    global_path: &Path,
    module: ConfigModule,
    cfg: &Config,
) -> Result<PathBuf> {
    let path = module.path(global_path)?;
    kit::config::write_reference_in_profile(
        global_path.parent().context("config path has no parent")?,
        module.file_name(),
        default_module(module),
    )?;
    if path.exists() {
        return Ok(path);
    }
    let mut text = format!("# {} — changes apply on Reload.\n", module.file_name());
    macro_rules! bindings {
        ($items:expr) => {
            for b in $items {
                text.push_str(&format!("\"{}\" = \"{}\"\n", b.combo, b.action.as_str()));
            }
        };
    }
    match module {
        ConfigModule::Rectangle => {
            bindings!(&cfg.modules.rectangle.bindings);
            text.push_str(&format!("gap = {}\n", cfg.modules.rectangle.settings.gap));
        }
        ConfigModule::Resize => bindings!(&cfg.modules.resize.bindings),
        ConfigModule::Scale => bindings!(&cfg.modules.scale.bindings),
        ConfigModule::MouseDrag => {
            let m = cfg.modules.mouse_drag.settings.drag_modifier;
            let mut parts = Vec::new();
            if m.ctrl {
                parts.push("ctrl");
            }
            if m.shift {
                parts.push("shift");
            }
            if m.alt {
                parts.push("alt");
            }
            if m.win {
                parts.push("win");
            }
            text.push_str(&format!("dragModifier = \"{}\"\n", parts.join("+")));
        }
        ConfigModule::OnTopBottom => {
            bindings!(&cfg.modules.on_top_bottom.bindings);
            text.push_str(&format!(
                "bottomReassertMs = {}\n",
                cfg.modules.on_top_bottom.settings.bottom_reassert_ms
            ));
        }
        ConfigModule::MaximizeMinimize => bindings!(&cfg.modules.maximize_minimize.bindings),
        ConfigModule::AppSwitch => bindings!(&cfg.modules.app_switch.bindings),
        ConfigModule::AppToggle => {
            text = window_app_toggle::config::render(&cfg.modules.app_toggle);
        }
        ConfigModule::QuickSwitch => {
            bindings!(&cfg.modules.quick_switch.bindings);
            text.push_str(&format!(
                "quickSwitch = {}\n",
                cfg.modules.quick_switch.settings.enabled
            ));
        }
        ConfigModule::VirtualDesktops => bindings!(&cfg.modules.virtual_desktops.bindings),
    }
    let patch = patch_with_removed_bindings(default_module(module), &text)?;
    std::fs::write(&path, patch).with_context(|| format!("cannot create {}", path.display()))?;
    Ok(path)
}

fn actions_text(bindings: &[Binding]) -> String {
    let mut text =
        String::from("# Global Window actions; feature bindings live in their own files.\n");
    for binding in bindings {
        let action = match binding.action {
            Action::Undo => "undo",
            Action::Quit => "quit",
            _ => continue,
        };
        text.push_str(&format!("\"{}\" = \"{}\"\n", binding.combo, action));
    }
    text
}

fn default_module(module: ConfigModule) -> &'static str {
    match module {
        ConfigModule::Rectangle => window_rectangle::config::DEFAULT_CONFIG,
        ConfigModule::Resize => window_resize::config::DEFAULT_CONFIG,
        ConfigModule::Scale => window_resize::config::DEFAULT_SCALE_CONFIG,
        ConfigModule::MouseDrag => window_mouse_drag::config::DEFAULT_CONFIG,
        ConfigModule::OnTopBottom => window_on_top_bottom::config::DEFAULT_CONFIG,
        ConfigModule::MaximizeMinimize => window_maximize_minimize::config::DEFAULT_CONFIG,
        ConfigModule::AppSwitch => window_app_switch::config::DEFAULT_CONFIG,
        ConfigModule::AppToggle => window_app_toggle::config::DEFAULT_CONFIG,
        ConfigModule::QuickSwitch => window_quick_switch::config::DEFAULT_CONFIG,
        ConfigModule::VirtualDesktops => window_virtual_desktops::config::DEFAULT_CONFIG,
    }
}

fn archive_legacy(path: &Path) -> Result<()> {
    let base = path.with_file_name("window-tools.legacy.toml");
    let mut backup = base.clone();
    for index in 1.. {
        if !backup.exists() {
            break;
        }
        backup = path.with_file_name(format!("window-tools.legacy-{index}.toml"));
    }
    std::fs::rename(path, &backup).with_context(|| {
        format!(
            "cannot archive legacy config {} as {}",
            path.display(),
            backup.display()
        )
    })
}

fn migrate_legacy(path: &Path) -> Result<()> {
    let legacy_path = path.with_file_name(LEGACY_CONFIG_NAME);
    if !legacy_path.exists() {
        return Ok(());
    }
    let text = std::fs::read_to_string(&legacy_path)
        .with_context(|| format!("cannot read {}", legacy_path.display()))?;
    let legacy = parse(&text)?;
    for module in MODULES {
        ensure_module_config(path, module, &legacy)?;
    }
    if !path.exists() {
        let text = patch_with_removed_bindings(DEFAULT_CONFIG, &actions_text(&legacy.global))?;
        std::fs::write(path, text).with_context(|| format!("cannot create {}", path.display()))?;
    }
    archive_legacy(&legacy_path)
}

fn migrate_scale(path: &Path) -> Result<()> {
    let target = ConfigModule::Scale.path(path)?;
    if target.exists() {
        return Ok(());
    }
    let mut combo = Combo::from_str("ctrl+alt+z")?;
    let source = path.with_file_name("screen-actions.toml");
    if source.exists() {
        let text = std::fs::read_to_string(&source)
            .with_context(|| format!("cannot read {}", source.display()))?;
        let value = text
            .parse::<toml::Value>()
            .with_context(|| format!("invalid {}", source.display()))?;
        if let Some(custom) = value
            .get("bindings")
            .and_then(|v| v.get("scale"))
            .and_then(toml::Value::as_str)
        {
            combo = Combo::from_str(custom)
                .with_context(|| format!("invalid scale binding in {}", source.display()))?;
        }
    }
    let effective = format!("# Fullscreen window zoom.\n\"{combo}\" = \"scale\"\n");
    let patch =
        patch_with_removed_bindings(window_resize::config::DEFAULT_SCALE_CONFIG, &effective)?;
    std::fs::write(&target, patch).with_context(|| format!("cannot create {}", target.display()))
}

/// Create split files, migrate any old aggregate once, then load only split files.
pub fn load(path: &Path) -> Result<Config> {
    migrate_scale(path)?;
    migrate_legacy(path)?;
    if !path.exists() {
        std::fs::write(
            path,
            kit::config::user_hotkey_patch_toml(DEFAULT_CONFIG, DEFAULT_CONFIG)?,
        )
        .with_context(|| format!("cannot create {}", path.display()))?;
    }
    for module in MODULES {
        let module_path = module.path(path)?;
        let default = default_module(module);
        kit::config::write_reference_in_profile(
            path.parent().context("config path has no parent")?,
            module.file_name(),
            default,
        )?;
        if !module_path.exists() {
            std::fs::write(
                &module_path,
                kit::config::user_hotkey_patch_toml(default, default)?,
            )
            .with_context(|| format!("cannot create {}", module_path.display()))?;
        }
    }
    reload(path)
}

/// Re-read every split file before replacing any active config.
pub fn reload(path: &Path) -> Result<Config> {
    let user =
        std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    let text = kit::config::merge_hotkey_toml(DEFAULT_CONFIG, &user)?;
    let global = parse(&text)?.global;
    load_modules(path, global)
}

/// Latest mtime across the global and existing module files.
pub fn modified(path: &Path) -> Option<SystemTime> {
    std::iter::once(path.to_path_buf())
        .chain(MODULES.iter().filter_map(|m| m.path(path).ok()))
        .filter_map(|p| std::fs::metadata(p).ok()?.modified().ok())
        .max()
}

/// One message listing every hotkey that could not be registered.
pub fn conflict_message(failed: &[String]) -> Option<String> {
    if failed.is_empty() {
        return None;
    }
    Some(format!(
        "These hotkeys could not be registered (in use by another process, or unsupported):\n\n  - {}\n\nClose the other process or edit the relevant window-*.toml file.",
        failed.join("\n  - ")
    ))
}

/// Parse the flat TOML table. Bad entries are reported to stderr and skipped:
/// one malformed binding must not take down the daemon.
pub fn parse(text: &str) -> Result<Config> {
    let value: toml::Value = text.parse().context("config is not valid TOML")?;
    let table = value
        .as_table()
        .context("config root must be a TOML table of `combo = \"action\"`")?;
    let mut out = Vec::new();
    let mut settings = Settings::default();
    for (combo, action) in table {
        if combo == "gap" {
            match action.as_integer() {
                Some(n) => settings.gap = n.clamp(0, 4096) as i32,
                None => eprintln!("warn: `gap` must be an integer; using 0"),
            }
            continue;
        }
        if combo == "quickSwitch" {
            match action.as_bool() {
                Some(on) => settings.quick_switch = on,
                None => eprintln!("warn: `quickSwitch` must be true or false; using true"),
            }
            continue;
        }
        if combo == "bottomReassertMs" {
            match action.as_integer() {
                // Below 100 ms the Z-order spam outweighs the tighter pinning.
                Some(n) => settings.bottom_reassert_ms = n.clamp(100, 60_000) as u64,
                None => eprintln!("warn: `bottomReassertMs` must be an integer; using 1000"),
            }
            continue;
        }
        if combo == "dragModifier" {
            match action.as_str() {
                Some(s) => match s.parse::<Mods>() {
                    Ok(m) => settings.drag_modifier = m,
                    Err(e) => eprintln!("warn: `dragModifier`: {e}; using win+alt"),
                },
                None => eprintln!("warn: `dragModifier` must be a string; using win+alt"),
            }
            continue;
        }
        if action.as_bool() == Some(false) || action.as_str() == Some("none") {
            continue;
        }
        if let Some(t) = action.as_str() {
            if t == "suppress" || t == "toggleScreen" {
                continue; // Owned by keyboard-shell and screen-power respectively.
            }
            match (combo.parse::<Combo>(), t.parse::<Action>()) {
                (Ok(c), Ok(a)) => out.push(Binding {
                    combo: c,
                    action: a,
                }),
                (Err(e), _) => eprintln!("warn: skipping binding `{combo}`: {e}"),
                (_, Err(e)) => eprintln!("warn: skipping binding `{combo}`: {e}"),
            }
        } else {
            eprintln!("warn: skipping binding `{combo}`: action must be a string");
        }
    }
    // TOML tables iterate in key order, not file order; sort so hotkey ids
    // are stable across runs and edits.
    out.sort_by_key(|b| b.combo);
    let global = out
        .iter()
        .copied()
        .filter(|b| matches!(b.action, Action::Undo | Action::Quit))
        .collect();
    assemble(legacy_modules(&out, settings), global)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::Key;

    #[test]
    fn parses_default_config() {
        let cfg = parse(LEGACY_DEFAULT_CONFIG).unwrap();
        assert_eq!(cfg.bindings.len(), 41);
        assert_eq!(cfg.settings.gap, 0);
        assert!(cfg.settings.quick_switch);
        assert!(cfg.settings.drag_modifier.win && !cfg.settings.drag_modifier.alt);
        let b = cfg
            .bindings
            .iter()
            .find(|b| b.action == Action::MoveToLeft)
            .expect("moveToLeft binding");
        assert_eq!(b.combo.key, Key::Char('h'));
        assert!(b.combo.win && b.combo.alt);
    }

    #[test]
    fn ignores_actions_owned_by_shell_and_screen() {
        assert_eq!("suppress".parse::<Action>().unwrap(), Action::Suppress);
        let cfg = parse(
            "\"win+f2\" = \"toggleScreen\"\n\"win+e\" = \"suppress\"\n\"alt+`\" = \"nextWindow\"\n",
        )
        .unwrap();
        assert_eq!(cfg.bindings.len(), 1);
        assert_eq!(cfg.bindings[0].action, Action::NextWindow);
    }

    #[test]
    fn parses_gap_and_clamps_it() {
        let cfg = parse("gap = 10\n\"ctrl+alt+left\" = \"moveToLeft\"\n").unwrap();
        assert_eq!(cfg.settings.gap, 10);
        assert_eq!(cfg.bindings.len(), 1);

        assert_eq!(parse("gap = -5\n").unwrap().settings.gap, 0);
        assert_eq!(parse("gap = 999999\n").unwrap().settings.gap, 4096);
        // Wrong type: warn, keep the default, still accept bindings.
        assert_eq!(
            parse("gap = \"wide\"\n\"ctrl+a\" = \"undo\"\n")
                .unwrap()
                .settings
                .gap,
            0
        );
    }

    #[test]
    fn parses_numbered_desktop_actions() {
        for n in 1..=9u8 {
            let cfg = parse(&format!("\"ctrl+alt+{n}\" = \"switchToDesktop{n}\"\n")).unwrap();
            assert_eq!(cfg.bindings[0].action, Action::SwitchToDesktop(n));
            let cfg = parse(&format!("\"ctrl+shift+{n}\" = \"moveToDesktop{n}\"\n")).unwrap();
            assert_eq!(cfg.bindings[0].action, Action::MoveToDesktop(n));
        }
        assert!("switchToDesktop0".parse::<Action>().is_err());
        assert!("switchToDesktop10".parse::<Action>().is_err());
        assert!("moveToDesktopX".parse::<Action>().is_err());
    }

    #[test]
    fn parses_quick_switch() {
        assert!(parse("").unwrap().settings.quick_switch);
        assert!(
            !parse("quickSwitch = false\n")
                .unwrap()
                .settings
                .quick_switch
        );
        // A wrong type keeps the default instead of failing the config.
        assert!(
            parse("quickSwitch = \"no\"\n")
                .unwrap()
                .settings
                .quick_switch
        );
    }

    #[test]
    fn parses_bottom_reassert_ms_and_clamps_it() {
        assert_eq!(parse("").unwrap().settings.bottom_reassert_ms, 250);
        assert_eq!(
            parse("bottomReassertMs = 250\n")
                .unwrap()
                .settings
                .bottom_reassert_ms,
            250
        );
        assert_eq!(
            parse("bottomReassertMs = 1\n")
                .unwrap()
                .settings
                .bottom_reassert_ms,
            100
        );
        assert_eq!(
            parse("bottomReassertMs = 999999\n")
                .unwrap()
                .settings
                .bottom_reassert_ms,
            60_000
        );
    }

    #[test]
    fn parses_drag_modifier() {
        let cfg = parse("dragModifier = \"win\"\n").unwrap();
        assert!(cfg.bindings.is_empty());
        assert!(cfg.settings.drag_modifier.win && !cfg.settings.drag_modifier.alt);

        // Missing setting: Win+Alt.
        let d = parse("").unwrap().settings.drag_modifier;
        assert!(d.win && d.alt);
        // Invalid value keeps the default instead of dropping the config.
        let d = parse("dragModifier = \"hyper\"\n")
            .unwrap()
            .settings
            .drag_modifier;
        assert!(d.win && d.alt);
    }

    #[test]
    fn default_resize_file_parses() {
        let p = parse_presets(include_str!("../resize.toml")).unwrap();
        assert_eq!(p.len(), 10);
        assert_eq!(p[0].to_string(), "1600x1200");
        assert_eq!(p[5].to_string(), "3/4");
    }

    #[test]
    fn parses_resize_presets_in_order() {
        let p = parse_presets("presets = [\"1600x1200\", \"1/2\", \"3/4\", \"bogus\"]\n").unwrap();
        assert_eq!(p.len(), 3);
        assert_eq!(p[0].to_string(), "1600x1200");
        assert_eq!(p[1].to_string(), "1/2");
        assert_eq!(p[2].to_string(), "3/4");

        assert!(parse_presets("nope = []\n").is_err());
        assert!(parse_presets("presets = \"1/2\"\n").is_err());
    }

    #[test]
    fn skips_bad_entries_and_continues() {
        let text = "\"ctrl+alt+left\" = \"moveToLeft\"\n\"bogus\" = \"moveToLeft\"\n\"ctrl+a\" = \"noSuchAction\"\n";
        let cfg = parse(text).unwrap();
        assert_eq!(cfg.bindings.len(), 1);
        assert_eq!(cfg.bindings[0].action, Action::MoveToLeft);
    }

    #[test]
    fn conflict_message_lists_every_failure_once() {
        assert_eq!(conflict_message(&[]), None);
        let msg = conflict_message(&[
            "ctrl+alt+left".to_string(),
            "win+f2 (unknown key)".to_string(),
        ])
        .expect("message");
        assert_eq!(msg.matches("ctrl+alt+left").count(), 1);
        assert!(msg.contains("win+f2 (unknown key)"));
    }

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("desktop-tools-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn module_default(module: ConfigModule) -> &'static str {
        match module {
            ConfigModule::Rectangle => window_rectangle::config::DEFAULT_CONFIG,
            ConfigModule::Resize => window_resize::config::DEFAULT_CONFIG,
            ConfigModule::Scale => window_resize::config::DEFAULT_SCALE_CONFIG,
            ConfigModule::MouseDrag => window_mouse_drag::config::DEFAULT_CONFIG,
            ConfigModule::OnTopBottom => window_on_top_bottom::config::DEFAULT_CONFIG,
            ConfigModule::MaximizeMinimize => window_maximize_minimize::config::DEFAULT_CONFIG,
            ConfigModule::AppSwitch => window_app_switch::config::DEFAULT_CONFIG,
            ConfigModule::AppToggle => window_app_toggle::config::DEFAULT_CONFIG,
            ConfigModule::QuickSwitch => window_quick_switch::config::DEFAULT_CONFIG,
            ConfigModule::VirtualDesktops => window_virtual_desktops::config::DEFAULT_CONFIG,
        }
    }

    #[test]
    fn module_defaults_match_legacy_defaults() {
        let dir = test_dir("module-defaults");
        let global = dir.join(CONFIG_NAME);
        std::fs::write(&global, LEGACY_DEFAULT_CONFIG).unwrap();
        for module in MODULES {
            std::fs::write(module.path(&global).unwrap(), module_default(module)).unwrap();
        }
        let mut split = load(&global).unwrap();
        assert!(split.bindings.iter().any(|b| b.action == Action::Scale));
        split.bindings.retain(|b| {
            b.action != Action::Scale
                && !matches!(
                    b.action,
                    Action::ToggleWezTerm | Action::ToggleDoubleCommander
                )
        });
        let legacy = parse(LEGACY_DEFAULT_CONFIG).unwrap();
        assert_eq!(split.bindings, legacy.bindings);
        assert_eq!(split.settings.gap, legacy.settings.gap);
        assert_eq!(split.settings.drag_modifier, legacy.settings.drag_modifier);
        assert_eq!(split.settings.quick_switch, legacy.settings.quick_switch);
        assert_eq!(
            split.settings.bottom_reassert_ms,
            legacy.settings.bottom_reassert_ms
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn legacy_aggregate_migrates_to_split_files_and_backup() {
        let dir = test_dir("module-overlay");
        let global = dir.join(CONFIG_NAME);
        let legacy_path = dir.join(LEGACY_CONFIG_NAME);
        let original = "gap = 7\n\"ctrl+alt+x\" = \"moveToLeft\"\n\"ctrl+alt+y\" = \"undo\"\n\"win+e\" = \"suppress\"\n";
        std::fs::write(&legacy_path, original).unwrap();
        load(&global).unwrap();
        let path = ConfigModule::Rectangle.path(&global).unwrap();
        assert!(!legacy_path.exists());
        assert_eq!(
            std::fs::read_to_string(dir.join("window-tools.legacy.toml")).unwrap(),
            original
        );
        assert!(std::fs::read_to_string(&global)
            .unwrap()
            .contains("ctrl+alt+y"));
        assert!(std::fs::read_to_string(&path).unwrap().contains("gap = 7"));
        std::fs::write(&path, "gap = 9\n\"ctrl+alt+x\" = \"moveToRight\"\n").unwrap();
        let updated = reload(&global).unwrap();
        assert_eq!(updated.settings.gap, 9);
        assert!(updated
            .bindings
            .iter()
            .any(|b| b.action == Action::MoveToRight));
        assert!(!updated
            .bindings
            .iter()
            .any(|b| b.action == Action::Suppress));
        std::fs::write(&path, "\"ctrl+alt+x\" = \"unknownAction\"\n").unwrap();
        assert!(reload(&global).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fresh_profile_uses_split_module_defaults() {
        let dir = test_dir("fresh-split");
        let global = dir.join(CONFIG_NAME);
        let cfg = load(&global).unwrap();
        assert_eq!(cfg.bindings.len(), 44);
        assert!(!std::fs::read_to_string(&global)
            .unwrap()
            .contains("swapWindows"));
        for module in MODULES {
            assert!(module.path(&global).unwrap().exists());
            assert!(dir.join("defaults").join(module.file_name()).exists());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scale_imports_screen_binding_without_overwriting_resize() {
        let dir = test_dir("scale-import");
        let global = dir.join(CONFIG_NAME);
        let resize = ConfigModule::Resize.path(&global).unwrap();
        let original = "\"ctrl+alt+win+s\" = \"sizerMenu\"\n";
        std::fs::write(&resize, original).unwrap();
        let source = dir.join("screen-actions.toml");
        std::fs::write(&source, "[bindings]\nscale = \"ctrl+alt+win+z\"\n").unwrap();
        let cfg = load(&global).unwrap();
        assert!(cfg.bindings.iter().any(|b| {
            b.action == Action::Scale && b.combo == Combo::from_str("ctrl+alt+win+z").unwrap()
        }));
        assert_eq!(std::fs::read_to_string(&resize).unwrap(), original);
        assert!(source.exists());
        std::fs::write(
            ConfigModule::Scale.path(&global).unwrap(),
            "\"ctrl+alt+win+y\" = \"scale\"\n",
        )
        .unwrap();
        let cfg = reload(&global).unwrap();
        assert!(cfg.bindings.iter().any(|b| {
            b.action == Action::Scale && b.combo == Combo::from_str("ctrl+alt+win+y").unwrap()
        }));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn migration_preserves_a_removed_nested_app_toggle_binding() {
        let default = default_module(ConfigModule::AppToggle);
        let mut effective: toml::Value = toml::from_str(default).unwrap();
        effective["bindings"]
            .as_table_mut()
            .unwrap()
            .remove("win+e")
            .unwrap();
        let patch =
            patch_with_removed_bindings(default, &toml::to_string_pretty(&effective).unwrap())
                .unwrap();
        let patch: toml::Value = toml::from_str(&patch).unwrap();
        assert_eq!(patch["bindings"]["win+e"].as_bool(), Some(false));
        assert_eq!(patch["bindings"].as_table().unwrap().len(), 1);
    }

    #[test]
    fn reload_reads_changed_module_file() {
        let dir = test_dir("reload-module");
        let global = dir.join(CONFIG_NAME);
        load(&global).unwrap();
        let path = ConfigModule::QuickSwitch.path(&global).unwrap();
        std::fs::write(
            &path,
            "quickSwitch = true\n\"ctrl+alt+l\" = \"none\"\n\"ctrl+alt+o\" = \"fileDialog\"\n",
        )
        .unwrap();
        let cfg = reload(&global).unwrap();
        let changed: Combo = "ctrl+alt+o".parse().unwrap();
        let original: Combo = "ctrl+alt+l".parse().unwrap();
        assert!(cfg
            .modules
            .quick_switch
            .bindings
            .iter()
            .any(|b| b.combo == changed));
        assert!(!cfg
            .modules
            .quick_switch
            .bindings
            .iter()
            .any(|b| b.combo == original));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn duplicate_hotkeys_across_module_files_are_rejected() {
        let dir = test_dir("module-conflict");
        let global = dir.join(CONFIG_NAME);
        std::fs::write(&global, "\"ctrl+alt+x\" = \"undo\"\n").unwrap();
        std::fs::write(
            ConfigModule::Rectangle.path(&global).unwrap(),
            "\"ctrl+alt+x\" = \"moveToLeft\"\n",
        )
        .unwrap();
        let error = match load(&global) {
            Ok(_) => panic!("duplicate hotkey unexpectedly accepted"),
            Err(e) => e.to_string(),
        };
        assert!(error.contains("more than one module"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
