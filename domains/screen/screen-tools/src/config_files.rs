use crate::config::{Action, Bindings, Config, MaskMode, MaskSize, OcrBackend, PinBorder};
use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const ACTIONS_FILE: &str = "screen-actions.toml";
pub const LEGACY_FILE: &str = "screen-tools.toml";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigModule {
    Capture,
    Draw,
    Ocr,
    Power,
    Record,
}

pub const MODULES: [ConfigModule; 5] = [
    ConfigModule::Capture,
    ConfigModule::Draw,
    ConfigModule::Ocr,
    ConfigModule::Power,
    ConfigModule::Record,
];

impl ConfigModule {
    pub fn label(self) -> &'static str {
        match self {
            Self::Capture => "Capture",
            Self::Draw => "Draw",
            Self::Ocr => "OCR",
            Self::Power => "Power",
            Self::Record => "Record",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Capture => "Copy or save the pixels inside the current Frame",
            Self::Draw => "Place, resize, move and pin a screen Frame",
            Self::Ocr => "Recognize Frame text and display or copy the result",
            Self::Power => "Toggle monitor power and restore windows after reconnect",
            Self::Record => "Record the Frame with FFmpeg and optional desktop audio",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Capture => "capture",
            Self::Draw => "draw",
            Self::Ocr => "ocr",
            Self::Power => "power",
            Self::Record => "record",
        }
    }

    pub fn file(self) -> &'static str {
        match self {
            Self::Capture => "screen-capture.toml",
            Self::Draw => "screen-draw.toml",
            Self::Ocr => "screen-ocr.toml",
            Self::Power => "screen-power.toml",
            Self::Record => "screen-record.toml",
        }
    }

    pub fn path(self, actions: &Path) -> PathBuf {
        actions.with_file_name(self.file())
    }
}

fn quote(text: &str) -> String {
    toml::Value::String(text.to_string()).to_string()
}

fn binding_lines(bindings: &Bindings, actions: &[Action]) -> String {
    let mut text = String::from("[bindings]\n");
    for &action in actions {
        text.push_str(&format!(
            "{} = {}\n",
            action.name(),
            quote(&action.get(bindings).to_string())
        ));
    }
    text
}

fn actions_text(cfg: &Config) -> String {
    format!(
        "# Screen actions awaiting their P6c feature owners.\n\n[external]\nflameshot = {}\n",
        quote(&cfg.flameshot)
    )
}

fn module_text(module: ConfigModule, cfg: &Config) -> String {
    match module {
        ConfigModule::Capture => format!(
            "# Capture output and the external image editor.\n\n[capture]\ndir = {}\n\n[external]\neditor = {}\n\n{}",
            quote(&cfg.capture_dir),
            quote(&cfg.external_editor),
            binding_lines(&cfg.bindings, &[Action::Capture])
        ),
        ConfigModule::Draw => {
            let border = match cfg.pin_border {
                PinBorder::Hatch => "hatch",
                PinBorder::Line => "line",
                PinBorder::None => "none",
            };
            let font = cfg
                .font
                .as_deref()
                .map(|font| format!("font = {}\n", quote(font)))
                .unwrap_or_default();
            format!(
                "# Frame, selection and pinned-image presentation.\n\n[frame]\nwidth = {}\nheight = {}\nshow_hint = {}\nwork_area = {}\n{}\n[pin]\nborder = {}\nhatch-color = {}\nline-color = {}\n\n{}",
                cfg.frame_w,
                cfg.frame_h,
                cfg.show_hint,
                cfg.work_area,
                font,
                quote(border),
                quote(&format!("#{:06x}", cfg.pin_hatch_color)),
                quote(&format!("#{:06x}", cfg.pin_line_color)),
                binding_lines(
                    &cfg.bindings,
                    &[
                        Action::Draw,
                        Action::Resize,
                        Action::Pin,
                        Action::FrameWindow,
                        Action::PinShot,
                        Action::Move,
                    ],
                )
            )
        }
        ConfigModule::Ocr => {
            let engine = match cfg.ocr_engine {
                OcrBackend::Auto => "auto",
                OcrBackend::Rapid => "rapid",
                OcrBackend::Tesseract => "tesseract",
            };
            let language = cfg.ocr_language.as_deref().unwrap_or("auto");
            let fill = match cfg.mask_fill {
                crate::config::MaskFill::Lines => "lines",
                crate::config::MaskFill::Boxes => "boxes",
            };
            let mode = match cfg.mask_mode {
                MaskMode::Box => "box",
                MaskMode::BoxMask => "box+mask",
                MaskMode::BoxOutside => "box+outside",
            };
            let size = match cfg.mask_size {
                MaskSize::Auto => quote("auto"),
                MaskSize::Pt(size) => size.to_string(),
            };
            format!(
                "# OCR engines, result presentation and notifications.\n\n[ocr]\nengine = {}\nscale = {}\nlanguage = {}\nauto_delay_ms = {}\n\n[mask]\nmode = {}\nfill = {}\nfont = {}\nsize = {}\n\n[notify]\nenabled = {}\n\n{}",
                quote(engine),
                cfg.ocr_scale,
                quote(language),
                cfg.ocr_auto.delay_ms,
                quote(mode),
                quote(fill),
                quote(&cfg.mask_font),
                size,
                cfg.notify_enabled,
                binding_lines(
                    &cfg.bindings,
                    &[Action::Toggle, Action::QuickOcr, Action::AutoOcr],
                )
            )
        }
        ConfigModule::Power => format!(
            "# Monitor power and multi-monitor window recovery.\n\n{}",
            binding_lines(&cfg.bindings, &[Action::ToggleScreen])
        ),
        ConfigModule::Record => format!(
            "# FFmpeg video, optional desktop audio and recording safeguards.\n\n[record]\ndir = {}\nname = {}\nfps = {}\nffmpeg = {}\naudio = {}\ncrf = {}\nmax_minutes = {}\nmin_free_mb = {}\n\n{}",
            quote(&cfg.record_dir),
            quote(&cfg.record_name),
            cfg.record_fps,
            quote(&cfg.record_ffmpeg),
            cfg.record_audio,
            cfg.record_crf,
            cfg.record_max_minutes,
            cfg.record_min_free_mb,
            binding_lines(&cfg.bindings, &[Action::Record])
        ),
    }
}

fn archive_legacy(path: &Path) -> Result<()> {
    let base = path.with_file_name("screen-tools.legacy.toml");
    let mut backup = base.clone();
    for index in 1.. {
        if !backup.exists() {
            break;
        }
        backup = path.with_file_name(format!("screen-tools.legacy-{index}.toml"));
    }
    std::fs::rename(path, &backup).with_context(|| {
        format!(
            "cannot archive legacy config {} as {}",
            path.display(),
            backup.display()
        )
    })
}

fn migrate_window_power(actions: &Path) -> Result<()> {
    let target = ConfigModule::Power.path(actions);
    if target.exists() {
        return Ok(());
    }
    let mut cfg = Config::default();
    let legacy = actions.with_file_name("window-actions.toml");
    if legacy.exists() {
        let text = std::fs::read_to_string(&legacy)
            .with_context(|| format!("cannot read {}", legacy.display()))?;
        let table: toml::Table =
            toml::from_str(&text).with_context(|| format!("cannot parse {}", legacy.display()))?;
        for (combo, action) in &table {
            if action.as_str() == Some("toggleScreen") {
                cfg.bindings.toggle_screen = combo
                    .parse()
                    .with_context(|| format!("invalid legacy toggleScreen binding: {combo}"))?;
                break;
            }
        }
    }
    if !target.exists() {
        std::fs::write(&target, module_text(ConfigModule::Power, &cfg))
            .with_context(|| format!("cannot write {}", target.display()))?;
    }
    Ok(())
}

fn migrate_legacy(actions: &Path) -> Result<()> {
    let legacy_path = actions.with_file_name(LEGACY_FILE);
    if !legacy_path.exists() {
        return Ok(());
    }
    let text = std::fs::read_to_string(&legacy_path)
        .with_context(|| format!("cannot read {}", legacy_path.display()))?;
    let legacy = crate::config::parse(&text)?;
    if !actions.exists() {
        std::fs::write(actions, actions_text(&legacy))
            .with_context(|| format!("cannot create {}", actions.display()))?;
    }
    for module in MODULES {
        let path = module.path(actions);
        if !path.exists() {
            std::fs::write(&path, module_text(module, &legacy))
                .with_context(|| format!("cannot create {}", path.display()))?;
        }
    }
    reload(actions)?; // archive only after the complete migrated configuration is valid
    archive_legacy(&legacy_path)
}

pub fn load(actions: &Path) -> Result<Config> {
    migrate_window_power(actions)?;
    migrate_legacy(actions)?;
    if !actions.exists() {
        std::fs::write(actions, actions_text(&Config::default()))
            .with_context(|| format!("cannot create {}", actions.display()))?;
    }
    for module in MODULES {
        let path = module.path(actions);
        if !path.exists() {
            std::fs::write(&path, module_text(module, &Config::default()))
                .with_context(|| format!("cannot create {}", path.display()))?;
        }
    }
    reload(actions)
}

fn merge(module: ConfigModule, part: Config, cfg: &mut Config) {
    match module {
        ConfigModule::Capture => {
            cfg.capture_dir = part.capture_dir;
            cfg.external_editor = part.external_editor;
            cfg.bindings.capture = part.bindings.capture;
        }
        ConfigModule::Draw => {
            cfg.frame_w = part.frame_w;
            cfg.frame_h = part.frame_h;
            cfg.work_area = part.work_area;
            cfg.show_hint = part.show_hint;
            cfg.font = part.font;
            cfg.pin_border = part.pin_border;
            cfg.pin_line_color = part.pin_line_color;
            cfg.pin_hatch_color = part.pin_hatch_color;
            cfg.bindings.draw = part.bindings.draw;
            cfg.bindings.resize = part.bindings.resize;
            cfg.bindings.pin = part.bindings.pin;
            cfg.bindings.frame_window = part.bindings.frame_window;
            cfg.bindings.pin_shot = part.bindings.pin_shot;
            cfg.bindings.r#move = part.bindings.r#move;
        }
        ConfigModule::Ocr => {
            cfg.ocr_scale = part.ocr_scale;
            cfg.ocr_engine = part.ocr_engine;
            cfg.ocr_auto = part.ocr_auto;
            cfg.ocr_language = part.ocr_language;
            cfg.mask_fill = part.mask_fill;
            cfg.mask_mode = part.mask_mode;
            cfg.mask_font = part.mask_font;
            cfg.mask_size = part.mask_size;
            cfg.notify_enabled = part.notify_enabled;
            cfg.bindings.toggle = part.bindings.toggle;
            cfg.bindings.quick_ocr = part.bindings.quick_ocr;
            cfg.bindings.auto_ocr = part.bindings.auto_ocr;
        }
        ConfigModule::Power => {
            cfg.bindings.toggle_screen = part.bindings.toggle_screen;
        }
        ConfigModule::Record => {
            cfg.record_dir = part.record_dir;
            cfg.record_name = part.record_name;
            cfg.record_fps = part.record_fps;
            cfg.record_ffmpeg = part.record_ffmpeg;
            cfg.record_audio = part.record_audio;
            cfg.record_crf = part.record_crf;
            cfg.record_max_minutes = part.record_max_minutes;
            cfg.record_min_free_mb = part.record_min_free_mb;
            cfg.bindings.record = part.bindings.record;
        }
    }
}

fn validate_bindings(bindings: &Bindings) -> Result<()> {
    let mut seen = HashMap::new();
    for action in Action::ALL {
        let combo = action.get(bindings);
        if let Some(previous) = seen.insert(combo, action) {
            bail!(
                "hotkey {combo} is assigned to both {} and {}",
                previous.name(),
                action.name()
            );
        }
    }
    Ok(())
}

/// Read every split file before replacing any active configuration.
pub fn reload(actions: &Path) -> Result<Config> {
    let text = std::fs::read_to_string(actions)
        .with_context(|| format!("cannot read {}", actions.display()))?;
    let root =
        crate::config::parse(&text).with_context(|| format!("invalid {}", actions.display()))?;
    let mut cfg = Config {
        flameshot: root.flameshot,
        bindings: Bindings {
            ..Bindings::default()
        },
        ..Config::default()
    };
    for module in MODULES {
        let path = module.path(actions);
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("cannot read {}", path.display()))?;
        let part =
            crate::config::parse(&text).with_context(|| format!("invalid {}", path.display()))?;
        merge(module, part, &mut cfg);
    }
    validate_bindings(&cfg.bindings)?;
    Ok(cfg)
}

pub fn modified(actions: &Path) -> Option<std::time::SystemTime> {
    std::iter::once(actions.to_path_buf())
        .chain(MODULES.iter().map(|module| module.path(actions)))
        .filter_map(|path| std::fs::metadata(path).ok()?.modified().ok())
        .max()
}

pub fn edit(module: ConfigModule, actions: &Path) {
    kit::win::edit_config(&module.path(actions));
}

pub fn edit_actions(actions: &Path) {
    kit::win::edit_config(actions);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "desktop-tools-screen-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn window_power_binding_imports_once_without_rewriting_window_config() {
        let dir = test_dir("power-migration");
        let actions = dir.join(ACTIONS_FILE);
        let window = dir.join("window-actions.toml");
        let original = "# keep me\n\"win+f3\" = \"toggleScreen\"\n\"ctrl+alt+win+u\" = \"undo\"\n";
        std::fs::write(&window, original).unwrap();

        let cfg = load(&actions).unwrap();
        assert_eq!(cfg.bindings.toggle_screen.to_string(), "win+f3");
        assert_eq!(std::fs::read_to_string(&window).unwrap(), original);

        std::fs::write(
            dir.join("screen-power.toml"),
            "[bindings]\ntoggle_screen = \"win+f4\"\n",
        )
        .unwrap();
        let cfg = load(&actions).unwrap();
        assert_eq!(cfg.bindings.toggle_screen.to_string(), "win+f4");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn fresh_profile_defaults_match_the_legacy_schema() {
        let dir = test_dir("split");
        let actions = dir.join(ACTIONS_FILE);
        let cfg = load(&actions).unwrap();
        assert_eq!(cfg, Config::default());
        assert!(!actions.with_file_name(LEGACY_FILE).exists());
        for module in MODULES {
            assert!(module.path(&actions).is_file());
        }
    }

    #[test]
    fn legacy_migrates_effective_values_without_overwriting_split_files() {
        let dir = test_dir("migrate");
        let actions = dir.join(ACTIONS_FILE);
        let legacy = dir.join(LEGACY_FILE);
        std::fs::write(
            &legacy,
            "[frame]\nwidth = 1234\n[ocr]\nscale = 2\n[record]\nfps = 24\n[bindings]\ndraw = \"ctrl+alt+F8\"\n",
        )
        .unwrap();
        std::fs::write(
            ConfigModule::Capture.path(&actions),
            "[capture]\ndir = \"keep\"\n",
        )
        .unwrap();
        let cfg = load(&actions).unwrap();
        assert_eq!(cfg.frame_w, 1234);
        assert_eq!(cfg.ocr_scale, 2);
        assert_eq!(cfg.record_fps, 24);
        assert_eq!(cfg.capture_dir, "keep");
        assert_eq!(cfg.bindings.draw.to_string(), "ctrl+alt+f8");
        assert!(!legacy.exists());
        assert!(dir.join("screen-tools.legacy.toml").is_file());
    }

    #[test]
    fn invalid_migration_leaves_legacy_active_until_the_conflict_is_fixed() {
        let dir = test_dir("migration-conflict");
        let actions = dir.join(ACTIONS_FILE);
        let legacy = dir.join(LEGACY_FILE);
        std::fs::write(&legacy, "[bindings]\ndraw = \"ctrl+alt+d\"\n").unwrap();
        std::fs::write(
            ConfigModule::Record.path(&actions),
            "[bindings]\nrecord = \"ctrl+alt+d\"\n",
        )
        .unwrap();
        assert!(load(&actions).is_err());
        assert!(legacy.is_file());
        assert!(!dir.join("screen-tools.legacy.toml").exists());
        std::fs::write(
            ConfigModule::Record.path(&actions),
            "[bindings]\nrecord = \"ctrl+alt+r\"\n",
        )
        .unwrap();
        assert!(load(&actions).is_ok());
        assert!(!legacy.exists());
        assert!(dir.join("screen-tools.legacy.toml").is_file());
    }

    #[test]
    fn cross_file_hotkey_conflicts_reject_the_whole_reload() {
        let dir = test_dir("conflict");
        let actions = dir.join(ACTIONS_FILE);
        load(&actions).unwrap();
        std::fs::write(
            ConfigModule::Record.path(&actions),
            "[bindings]\nrecord = \"ctrl+alt+d\"\n",
        )
        .unwrap();
        let error = reload(&actions).unwrap_err().to_string();
        assert!(error.contains("assigned to both"), "{error}");
    }

    #[test]
    fn invalid_one_module_keeps_reload_atomic() {
        let dir = test_dir("invalid");
        let actions = dir.join(ACTIONS_FILE);
        let before = load(&actions).unwrap();
        std::fs::write(ConfigModule::Ocr.path(&actions), "[ocr\n").unwrap();
        assert!(reload(&actions).is_err());
        assert_eq!(before, Config::default());
    }
}
