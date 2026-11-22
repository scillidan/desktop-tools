//! Split taskbar configuration with one-way legacy aggregate fallback.

use anyhow::{Context, Result};
use serde::{de::DeserializeOwned, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

pub const LEGACY_AGGREGATE: &str = "taskbar-tools.toml";
static LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub fn path(file_name: &str) -> Result<PathBuf> {
    kit::config::profile_file(file_name)
}

fn parse_module<T: DeserializeOwned>(text: &str, nested: Option<&str>) -> Result<T> {
    if let Some(section) = nested {
        let value: toml::Value = toml::from_str(text)?;
        if let Some(value) = value.get(section) {
            return T::deserialize(value.clone()).context("invalid legacy module section");
        }
    }
    Ok(toml::from_str(text)?)
}

fn write_patch(path: &Path, default: &str, effective: &str) -> Result<()> {
    let text = kit::config::user_patch_toml(default, effective)?;
    std::fs::write(path, text).with_context(|| format!("cannot write {}", path.display()))
}

/// Load a module file, falling back to its old aggregate section or old section name.
pub fn load<T>(file_name: &str, section: &str, nested: Option<&str>) -> Result<T>
where
    T: DeserializeOwned + Serialize + Default,
{
    let _guard = LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let path = path(file_name)?;
    let default = toml::to_string_pretty(&T::default())?;
    if path.exists() {
        let user = std::fs::read_to_string(&path)
            .with_context(|| format!("cannot read {}", path.display()))?;
        let text = kit::config::merge_toml(&default, &user)?;
        return parse_module(&text, nested).with_context(|| format!("invalid {}", path.display()));
    }

    let legacy_path = path.with_file_name(LEGACY_AGGREGATE);
    if legacy_path.exists() {
        let text = std::fs::read_to_string(&legacy_path)
            .with_context(|| format!("cannot read {}", legacy_path.display()))?;
        let value: toml::Value =
            toml::from_str(&text).with_context(|| format!("invalid {}", legacy_path.display()))?;
        if let Some(value) = value.get(section) {
            return T::deserialize(value.clone())
                .with_context(|| format!("invalid [{section}] in {}", legacy_path.display()));
        }
        return Ok(T::default());
    }

    let config = T::default();
    kit::config::write_reference_in_profile(
        path.parent().context("config path has no parent")?,
        file_name,
        &default,
    )?;
    write_patch(&path, &default, &default)?;
    Ok(config)
}

/// Create a module file from its effective legacy/default value, never overwrite it.
pub fn materialize<T>(file_name: &str, section: &str, nested: Option<&str>) -> Result<PathBuf>
where
    T: DeserializeOwned + Serialize + Default,
{
    let config = load::<T>(file_name, section, nested)?;
    let path = path(file_name)?;
    let default = toml::to_string_pretty(&T::default())?;
    kit::config::write_reference_in_profile(
        path.parent().context("config path has no parent")?,
        file_name,
        &default,
    )?;
    if !path.exists() {
        let effective = toml::to_string_pretty(&config)?;
        write_patch(&path, &default, &effective)?;
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
    struct Example {
        enabled: bool,
        value: u32,
    }

    #[test]
    fn module_file_reads_aggregate_then_becomes_independent() {
        let dir = std::env::temp_dir().join(format!("taskbar-config-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("module.toml");
        std::fs::write(
            dir.join(LEGACY_AGGREGATE),
            "[module]\nenabled = true\nvalue = 7\n",
        )
        .unwrap();
        let value: toml::Value =
            toml::from_str(&std::fs::read_to_string(dir.join(LEGACY_AGGREGATE)).unwrap()).unwrap();
        let config = Example::deserialize(value.get("module").unwrap().clone()).unwrap();
        assert_eq!(
            config,
            Example {
                enabled: true,
                value: 7
            }
        );
        std::fs::write(&path, toml::to_string_pretty(&config).unwrap()).unwrap();
        assert_eq!(
            toml::from_str::<Example>(&std::fs::read_to_string(path).unwrap()).unwrap(),
            config
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
