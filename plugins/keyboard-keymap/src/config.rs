use crate::engine::Engine;
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const FILE_NAME: &str = "keyboard-keymap.toml";
pub const DEFAULT_CONFIG: &str = include_str!("../keyboard-keymap.toml");

#[derive(Deserialize)]
#[serde(untagged)]
enum Replacement {
    Text(String),
    Disabled(bool),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileConfig {
    #[serde(default)]
    keymap: BTreeMap<String, Replacement>,
    // P4a briefly stored Shell Keys here; P4b preserves that file as a backup.
    #[serde(default)]
    #[allow(dead_code)]
    shell: Option<toml::Value>,
}

pub fn load() -> Result<(PathBuf, Engine)> {
    load_from(kit::config::profile_dir()?)
}

pub fn load_from(root: PathBuf) -> Result<(PathBuf, Engine)> {
    std::fs::create_dir_all(&root)?;
    kit::config::write_reference_in_profile(&root, FILE_NAME, DEFAULT_CONFIG)?;
    let path = root.join(FILE_NAME);
    if path.exists() {
        split_p4a_shell_config(&path)?;
    } else {
        std::fs::write(
            &path,
            kit::config::user_patch_toml(DEFAULT_CONFIG, DEFAULT_CONFIG)?,
        )?;
    }
    let user = std::fs::read_to_string(&path)?;
    let text = kit::config::merge_toml(DEFAULT_CONFIG, &user)?;
    let file: FileConfig =
        toml::from_str(&text).with_context(|| format!("cannot parse {}", path.display()))?;
    let mut replacements = BTreeMap::new();
    for (trigger, replacement) in file.keymap {
        match replacement {
            Replacement::Text(text) => {
                replacements.insert(trigger, text);
            }
            Replacement::Disabled(false) => {}
            Replacement::Disabled(true) => bail!("keymap `{trigger}` must be text or false"),
        }
    }
    let engine = Engine::new(replacements)
        .with_context(|| format!("invalid keymap in {}", path.display()))?;
    Ok((path, engine))
}

fn split_p4a_shell_config(path: &Path) -> Result<()> {
    let text = std::fs::read_to_string(path)?;
    let Ok(file) = toml::from_str::<FileConfig>(&text) else {
        return Ok(());
    };
    if !file.keymap.is_empty() || file.shell.is_none() {
        return Ok(());
    }
    let backup = path.with_file_name("keyboard-keymap.legacy-shell.toml");
    if backup.exists() {
        return Ok(());
    }
    std::fs::rename(path, &backup)
        .with_context(|| format!("cannot preserve old shell config as {}", backup.display()))?;
    std::fs::write(
        path,
        kit::config::user_patch_toml(DEFAULT_CONFIG, DEFAULT_CONFIG)?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("keyboard-keymap-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn creates_static_default_config() {
        let dir = test_dir("default");
        let (_, mut engine) = load_from(dir.clone()).unwrap();
        assert_eq!(type_text(&mut engine, "#alpha").unwrap().replacement, "α");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn splits_only_the_p4a_shell_only_file_and_preserves_it() {
        let dir = test_dir("split");
        let path = dir.join(FILE_NAME);
        std::fs::write(&path, "[shell]\nsuppress = [\"win+e\"]\n").unwrap();
        let (_, mut engine) = load_from(dir.clone()).unwrap();
        assert!(path.exists());
        assert!(dir.join("keyboard-keymap.legacy-shell.toml").exists());
        assert_eq!(type_text(&mut engine, "#alpha").unwrap().replacement, "α");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn existing_mixed_config_is_not_rewritten() {
        let dir = test_dir("mixed");
        let path = dir.join(FILE_NAME);
        let text = "[shell]\nsuppress = [\"win+e\"]\n[keymap]\n\"#x\" = \"χ\"\n";
        std::fs::write(&path, text).unwrap();
        let (_, mut engine) = load_from(dir.clone()).unwrap();
        assert_eq!(std::fs::read_to_string(path).unwrap(), text);
        assert_eq!(type_text(&mut engine, "#x").unwrap().replacement, "χ");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn sparse_patch_inherits_defaults_and_false_disables_one() {
        let dir = test_dir("sparse");
        let path = dir.join(FILE_NAME);
        std::fs::write(&path, "[keymap]\n\"#alpha\" = false\n\"#x\" = \"χ\"\n").unwrap();
        let (_, mut engine) = load_from(dir.clone()).unwrap();
        assert_eq!(type_text(&mut engine, "#alpha"), None);
        assert_eq!(type_text(&mut engine, "#Alpha").unwrap().replacement, "Α");
        assert_eq!(type_text(&mut engine, "#x").unwrap().replacement, "χ");
        let _ = std::fs::remove_dir_all(dir);
    }

    fn type_text(engine: &mut Engine, text: &str) -> Option<crate::engine::Expansion> {
        text.chars().find_map(|ch| engine.push(ch))
    }
}
