use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use window_core::{Binding, Combo};

use crate::AppKind;

pub const DEFAULT_CONFIG: &str = include_str!("../window-app-toggle.toml");

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub exe: PathBuf,
    pub command: PathBuf,
    pub args: Vec<String>,
}

impl AppConfig {
    pub fn missing_executable(&self) -> Option<&Path> {
        if !self.exe.is_file() {
            Some(&self.exe)
        } else if !self.command.is_file() {
            Some(&self.command)
        } else {
            None
        }
    }
}

#[derive(Clone, Debug)]
pub struct Config {
    pub bindings: Vec<Binding<AppKind>>,
    pub wezterm: AppConfig,
    pub double_commander: AppConfig,
}

impl Default for Config {
    fn default() -> Self {
        parse(DEFAULT_CONFIG).expect("default app-toggle config")
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileConfig {
    #[serde(default)]
    bindings: BTreeMap<String, String>,
    wezterm: RawApp,
    #[serde(rename = "doubleCommander")]
    double_commander: RawApp,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawApp {
    exe: PathBuf,
    command: Option<PathBuf>,
    #[serde(default)]
    args: Vec<String>,
}

impl RawApp {
    fn parse(self) -> Result<AppConfig> {
        let exe = expand_user_profile(&self.exe);
        let command = expand_user_profile(self.command.as_ref().unwrap_or(&exe));
        if !exe.is_absolute() || !command.is_absolute() {
            bail!("app exe and command must be absolute paths");
        }
        Ok(AppConfig {
            exe,
            command,
            args: self.args,
        })
    }
}

fn expand_user_profile(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    let Ok(profile) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) else {
        return path.to_path_buf();
    };
    PathBuf::from(text.replacen("%USERPROFILE%", &profile, 1))
}

pub fn parse(text: &str) -> Result<Config> {
    let file: FileConfig = toml::from_str(text)?;
    let mut bindings = Vec::new();
    for (combo, action) in file.bindings {
        let combo = combo
            .parse::<Combo>()
            .with_context(|| format!("invalid app-toggle combo `{combo}`"))?;
        let action = match action.as_str() {
            "wezterm" => AppKind::WezTerm,
            "doubleCommander" => AppKind::DoubleCommander,
            _ => bail!("unknown app-toggle action `{action}`"),
        };
        bindings.push(Binding { combo, action });
    }
    Ok(Config {
        bindings,
        wezterm: file.wezterm.parse().context("invalid [wezterm]")?,
        double_commander: file
            .double_commander
            .parse()
            .context("invalid [doubleCommander]")?,
    })
}

fn quoted(text: impl Into<String>) -> String {
    toml::Value::String(text.into()).to_string()
}

fn render_app(name: &str, app: &AppConfig) -> String {
    let args = toml::Value::Array(app.args.iter().cloned().map(toml::Value::String).collect());
    format!(
        "[{name}]\nexe = {}\ncommand = {}\nargs = {args}\n",
        quoted(app.exe.to_string_lossy()),
        quoted(app.command.to_string_lossy()),
    )
}

pub fn render(config: &Config) -> String {
    let mut bindings = String::from("[bindings]\n");
    for binding in &config.bindings {
        let action = match binding.action {
            AppKind::WezTerm => "wezterm",
            AppKind::DoubleCommander => "doubleCommander",
        };
        bindings.push_str(&format!(
            "{} = {}\n",
            quoted(binding.combo.to_string()),
            quoted(action)
        ));
    }
    format!(
        "# Focus or launch applications. `exe` identifies the exact install.\n{}\n{}\n{}",
        bindings,
        render_app("wezterm", &config.wezterm),
        render_app("doubleCommander", &config.double_commander),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_have_expected_bindings_and_exact_paths() {
        let cfg = Config::default();
        assert_eq!(cfg.bindings.len(), 2);
        assert!(cfg
            .bindings
            .iter()
            .any(|b| b.combo == "ctrl+`".parse().unwrap() && b.action == AppKind::WezTerm));
        assert!(cfg
            .bindings
            .iter()
            .any(|b| b.combo == "win+e".parse().unwrap() && b.action == AppKind::DoubleCommander));
        assert!(cfg.wezterm.exe.is_absolute());
        assert!(cfg.double_commander.exe.is_absolute());
        assert_eq!(cfg.wezterm.args, ["start", "--cwd", "."]);
    }

    #[test]
    fn custom_apps_round_trip() {
        let cfg = parse(
            "[bindings]\n'ctrl+f9'='wezterm'\n'ctrl+f10'='doubleCommander'\n\
             [wezterm]\nexe='C:\\One\\one.exe'\nargs=['--new']\n\
             [doubleCommander]\nexe='C:\\Two\\two.exe'\n",
        )
        .unwrap();
        assert_eq!(cfg.wezterm.command, PathBuf::from("C:\\One\\one.exe"));
        let again = parse(&render(&cfg)).unwrap();
        assert_eq!(again.bindings.len(), 2);
        assert_eq!(again.wezterm.args, ["--new"]);
        assert_eq!(again.double_commander.exe, cfg.double_commander.exe);
    }

    #[test]
    fn missing_target_and_launcher_are_reported_before_binding() {
        let missing = std::env::temp_dir().join(format!(
            "desktop-tools-missing-app-toggle-{}",
            std::process::id()
        ));
        let existing = std::env::current_exe().unwrap();
        let mut app = AppConfig {
            exe: missing.clone(),
            command: missing.clone(),
            args: Vec::new(),
        };
        assert_eq!(app.missing_executable(), Some(missing.as_path()));
        app.exe = existing.clone();
        assert_eq!(app.missing_executable(), Some(missing.as_path()));
        app.command = existing;
        assert_eq!(app.missing_executable(), None);
    }

    #[test]
    fn rejects_relative_paths_and_unknown_actions() {
        assert!(
            parse("[wezterm]\nexe='one.exe'\n[doubleCommander]\nexe='C:\\Two\\two.exe'\n").is_err()
        );
        assert!(parse(
            "[bindings]\n'ctrl+f9'='other'\n\
             [wezterm]\nexe='C:\\One\\one.exe'\n\
             [doubleCommander]\nexe='C:\\Two\\two.exe'\n"
        )
        .is_err());
    }
}
