use anyhow::Result;

#[derive(Clone, Copy)]
pub enum Module {
    Center,
    Hidden,
    Pin,
}

/// Migrate the old aggregate once, then load only independent feature files.
pub fn load() -> Result<()> {
    migrate_legacy()?;
    // This entry runs at startup/reload, not in either worker's polling loop.
    let _ = taskbar_center::materialize()?;
    let _ = taskbar_hidden::materialize()?;
    let _ = taskbar_pin::materialize()?;
    taskbar_center::load()?.validate()?;
    let _ = taskbar_hidden::load()?;
    let _ = taskbar_pin::load()?;
    Ok(())
}

pub fn edit(module: Module) -> Result<()> {
    let path = match module {
        Module::Center => taskbar_center::materialize()?,
        Module::Hidden => taskbar_hidden::materialize()?,
        Module::Pin => taskbar_pin::materialize()?,
    };
    kit::win::edit_config(&path);
    Ok(())
}

fn migrate_legacy() -> Result<()> {
    let legacy = taskbar_core::config::path(taskbar_core::config::LEGACY_AGGREGATE)?;
    if !legacy.exists() {
        return Ok(());
    }
    let _ = taskbar_center::materialize()?;
    let _ = taskbar_hidden::materialize()?;
    let _ = taskbar_pin::materialize()?;
    let backup = unique_backup(&legacy);
    std::fs::rename(&legacy, &backup)?;
    eprintln!(
        "taskbar: migrated {} to independent feature files; backup {}",
        legacy.display(),
        backup.display()
    );
    Ok(())
}

fn unique_backup(legacy: &std::path::Path) -> std::path::PathBuf {
    let backup = legacy.with_extension("legacy.toml");
    if !backup.exists() {
        return backup;
    }
    for index in 1.. {
        let candidate = legacy.with_extension(format!("legacy-{index}.toml"));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_legacy_worker_defaults() {
        let center = taskbar_center::Config::default();
        let hidden = taskbar_hidden::Config::default();
        let pin = taskbar_pin::Config::default();
        assert_eq!(center.poll_ms, 100);
        assert_eq!(hidden.hotkey, "ctrl+alt+f9");
        assert!(hidden.resize_snapped);
        assert_eq!(pin.poll_ms, 750);
        assert!(pin.pin.is_empty());
    }

    #[test]
    fn backup_names_never_overwrite() {
        let dir = std::env::temp_dir().join(format!("taskbar-backup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let legacy = dir.join("taskbar-tools.toml");
        let first = unique_backup(&legacy);
        std::fs::write(&first, "first").unwrap();
        let second = unique_backup(&legacy);
        assert_ne!(first, second);
        assert_eq!(std::fs::read_to_string(first).unwrap(), "first");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
