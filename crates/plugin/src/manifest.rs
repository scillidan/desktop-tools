//! `plugin.toml` parsing and validation.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

pub const API_VERSION: u32 = 1;
pub const GROUPS: [(&str, &str); 4] = [
    ("window", "Window"),
    ("screen", "Screen"),
    ("keyboard", "Keyboard"),
    ("taskbar", "Taskbar"),
];

pub fn group_index(group: &str) -> Option<usize> {
    GROUPS.iter().position(|(id, _)| *id == group)
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub api: u32,
    pub exe: String,
    #[serde(default = "default_group")]
    pub group: String,
    /// External integrations are presented after native tools.
    #[serde(default)]
    pub external: bool,
    /// One-line function summary shown inside nested plugin menus.
    #[serde(default)]
    pub description: Option<String>,
    /// Whether the host adds its conventional aggregate config edit entry.
    #[serde(default = "default_config")]
    pub config: bool,
}

fn default_group() -> String {
    // Preserve packages built before the merged suite introduced groups.
    "keyboard".to_string()
}

fn default_config() -> bool {
    true
}

impl Manifest {
    pub fn load(path: &Path) -> Result<Manifest> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("cannot read {}", path.display()))?;
        let manifest: Manifest =
            toml::from_str(&text).with_context(|| format!("invalid TOML in {}", path.display()))?;
        let dir = path
            .parent()
            .context("plugin.toml has no parent directory")?;
        manifest.validate(dir)?;
        Ok(manifest)
    }

    /// Validate against the directory that contains `plugin.toml`.
    pub fn validate(&self, dir: &Path) -> Result<()> {
        if self.id.is_empty()
            || !self
                .id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            || self.id.starts_with('-')
            || self.id.ends_with('-')
        {
            bail!("invalid plugin id `{}`", self.id);
        }
        let dir_name = dir
            .file_name()
            .and_then(|s| s.to_str())
            .context("plugin directory has no UTF-8 name")?;
        if dir_name != self.id {
            bail!(
                "plugin id `{}` does not match directory `{dir_name}`",
                self.id
            );
        }
        if self.name.trim().is_empty() {
            bail!("plugin `{}` has an empty name", self.id);
        }
        if self.version.trim().is_empty() {
            bail!("plugin `{}` has an empty version", self.id);
        }
        if self.api != API_VERSION {
            bail!(
                "plugin `{}` requires api {}, host supports {API_VERSION}",
                self.id,
                self.api
            );
        }
        if group_index(&self.group).is_none() {
            bail!(
                "plugin `{}` has group `{}` (expected window|screen|keyboard|taskbar)",
                self.id,
                self.group
            );
        }
        validate_exe(dir, &self.exe)?;
        Ok(())
    }
}

fn validate_exe(dir: &Path, exe: &str) -> Result<()> {
    if exe.trim().is_empty() {
        bail!("plugin exe is empty");
    }
    let path = Path::new(exe);
    if path.is_absolute() {
        bail!("plugin exe `{exe}` must be relative");
    }
    let mut components = path.components();
    let only = components.next();
    if !matches!(only, Some(Component::Normal(_))) || components.next().is_some() {
        bail!("plugin exe `{exe}` must be a single file name (no traversal)");
    }
    let full: PathBuf = dir.join(path);
    if !full.is_file() {
        bail!("plugin exe `{}` does not exist", full.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("plugin-manifest-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(dir: &Path, exe: &str) -> PathBuf {
        std::fs::write(dir.join(exe), b"x").unwrap();
        let path = dir.join("plugin.toml");
        let id = dir.file_name().unwrap().to_string_lossy();
        std::fs::write(
            &path,
            format!(
                "id = \"{id}\"\nname = \"Test\"\nversion = \"0.1\"\napi = 1\nexe = \"{exe}\"\n"
            ),
        )
        .unwrap();
        path
    }

    #[test]
    fn valid_manifest_loads() {
        let dir = temp_dir("good").join("keyboard-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = write(&dir, "keyboard-test.exe");
        let m = Manifest::load(&path).unwrap();
        assert_eq!(m.id, "keyboard-test");
        assert_eq!(m.api, API_VERSION);
        assert_eq!(m.group, "keyboard");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn traversal_and_wrong_api_are_rejected() {
        let dir = temp_dir("bad").join("keyboard-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = write(&dir, "keyboard-test.exe");

        let mut m = Manifest::load(&path).unwrap();
        m.exe = "../keyboard-test.exe".into();
        assert!(m.validate(&dir).is_err());

        let mut m = Manifest::load(&path).unwrap();
        m.exe = "sub/keyboard-test.exe".into();
        assert!(m.validate(&dir).is_err());

        let mut m = Manifest::load(&path).unwrap();
        m.exe = "C:\\Windows\\notepad.exe".into();
        assert!(m.validate(&dir).is_err());

        let mut m = Manifest::load(&path).unwrap();
        m.api = 2;
        assert!(m.validate(&dir).is_err());

        let mut m = Manifest::load(&path).unwrap();
        m.group = "others".into();
        assert!(m.validate(&dir).is_err());

        let mut m = Manifest::load(&path).unwrap();
        m.id = "other".into();
        assert!(m.validate(&dir).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
