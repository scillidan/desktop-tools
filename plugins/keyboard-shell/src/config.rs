use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use kit::keys::Combo;
use serde::Deserialize;

pub const FILE_NAME: &str = "keyboard-shell.toml";
pub const DEFAULT_CONFIG: &str = include_str!("../keyboard-shell.toml");

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileConfig {
    shell: ShellConfig,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ShellConfig {
    suppress: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub suppress: Vec<Combo>,
}

pub fn parse(text: &str) -> Result<Config> {
    let file: FileConfig = toml::from_str(text)?;
    let mut seen = HashSet::new();
    let mut suppress = Vec::with_capacity(file.shell.suppress.len());
    for raw in file.shell.suppress {
        let combo: Combo = raw
            .parse()
            .with_context(|| format!("invalid shell suppression `{raw}`"))?;
        if !combo.win {
            bail!("shell suppression `{raw}` must contain Win");
        }
        if !seen.insert(combo) {
            bail!("duplicate shell suppression `{raw}`");
        }
        suppress.push(combo);
    }
    Ok(Config { suppress })
}

pub fn load() -> Result<(PathBuf, Config)> {
    load_in_profile(&kit::config::profile_dir()?)
}

pub fn load_in_profile(profile: &Path) -> Result<(PathBuf, Config)> {
    std::fs::create_dir_all(profile)
        .with_context(|| format!("cannot create {}", profile.display()))?;
    let path = profile.join(FILE_NAME);
    if !path.exists() {
        let text = import_previous_keymap(profile)?
            .or(import_window_suppressions(profile)?)
            .unwrap_or_else(|| DEFAULT_CONFIG.into());
        std::fs::write(&path, text).with_context(|| format!("cannot write {}", path.display()))?;
    }
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("cannot read {}", path.display()))?;
    Ok((path, parse(&text)?))
}

/// Preserve the short-lived P4a shell config when splitting it from Key Map.
fn import_previous_keymap(profile: &Path) -> Result<Option<String>> {
    let path = profile.join("keyboard-keymap.toml");
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("cannot read previous {}", path.display()))?;
    let raw: FileConfig = toml::from_str(&text)
        .with_context(|| format!("cannot parse previous {}", path.display()))?;
    Ok(Some(render(&raw.shell.suppress)))
}

/// Import effective Window suppressions once; never rewrite either file later.
fn import_window_suppressions(profile: &Path) -> Result<Option<String>> {
    let path = profile.join("window-tools.toml");
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("cannot read legacy {}", path.display()))?;
    let table: toml::Table =
        toml::from_str(&text).with_context(|| format!("cannot parse legacy {}", path.display()))?;
    let combos: Vec<String> = table
        .iter()
        .filter(|(_, value)| value.as_str() == Some("suppress"))
        .map(|(combo, _)| combo.clone())
        .collect();
    if combos.is_empty() {
        return Ok(None); // A fresh split Window config has nothing to import.
    }
    Ok(Some(render(&combos)))
}

fn render(combos: &[String]) -> String {
    let mut text = String::from(
        "# Windows shell keys are suppressed here, not by window-tools or Key Map.\n[shell]\nsuppress = [\n",
    );
    for combo in combos {
        let quoted = toml::Value::String(combo.clone()).to_string();
        text.push_str("    ");
        text.push_str(&quoted);
        text.push_str(",\n");
    }
    text.push_str("]\n");
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("keyboard-shell-{name}-{}", std::process::id()))
    }

    #[test]
    fn default_contains_every_legacy_shell_binding() {
        let cfg = parse(DEFAULT_CONFIG).unwrap();
        assert_eq!(cfg.suppress.len(), 67);
        assert!(cfg.suppress.contains(&"win+e".parse().unwrap()));
        assert!(cfg.suppress.contains(&"win+shift+=".parse().unwrap()));
        assert!(cfg.suppress.contains(&"win+prtscn".parse().unwrap()));
    }

    #[test]
    fn rejects_duplicate_or_non_win_bindings() {
        assert!(parse("[shell]\nsuppress=[\"win+e\",\"win+e\"]").is_err());
        assert!(parse("[shell]\nsuppress=[\"ctrl+c\"]").is_err());
    }

    #[test]
    fn fresh_split_window_config_uses_shell_defaults() {
        let dir = test_dir("fresh-window");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("window-tools.toml"), "\"ctrl+alt+u\"=\"undo\"").unwrap();
        let (_, cfg) = load_in_profile(&dir).unwrap();
        assert_eq!(cfg.suppress.len(), 67);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn imports_custom_window_suppressions_once() {
        let dir = test_dir("import");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("window-tools.toml"),
            "gap=4\n\"win+f12\"=\"suppress\"\n\"ctrl+alt+l\"=\"moveToLeft\"\n",
        )
        .unwrap();
        let (path, cfg) = load_in_profile(&dir).unwrap();
        assert_eq!(cfg.suppress, vec!["win+f12".parse().unwrap()]);
        std::fs::write(dir.join("window-tools.toml"), "\"win+e\"=\"suppress\"").unwrap();
        let (_, again) = load_in_profile(&dir).unwrap();
        assert_eq!(again.suppress, cfg.suppress);
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn preserves_previous_keymap_shell_settings_when_splitting() {
        let dir = test_dir("previous-keymap");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("keyboard-keymap.toml"),
            "[shell]\nsuppress = [\"win+e\", \"win+ctrl+d\"]\n",
        )
        .unwrap();
        let (_, cfg) = load_in_profile(&dir).unwrap();
        assert_eq!(cfg.suppress.len(), 2);
        assert!(cfg.suppress.contains(&"win+ctrl+d".parse().unwrap()));
        assert!(dir.join("keyboard-keymap.toml").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
