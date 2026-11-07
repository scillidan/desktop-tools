//! TOML config: profile resolution, first-run creation, one-time legacy import.
//!
//! The profile root is `--profile <dir>` > `DTTOOLS_PROFILE` >
//! legacy `KBTOOLS_PROFILE` > `<exe_dir>/Profile/User`, mirroring the
//! Keypirinha profile layout.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

pub const PROFILE_ENV: &str = "DTTOOLS_PROFILE";
pub const LEGACY_PROFILE_ENV: &str = "KBTOOLS_PROFILE";

/// Resolve the profile root without touching the filesystem or the process
/// environment. Kept pure so the resolution order is testable.
pub fn resolve_profile_dir(args: &[String], env: Option<&str>, exe_dir: &Path) -> Result<PathBuf> {
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--profile" {
            let dir = args.get(i + 1).context("--profile requires a directory")?;
            return Ok(PathBuf::from(dir));
        }
        if let Some(dir) = arg.strip_prefix("--profile=") {
            if dir.is_empty() {
                bail!("--profile requires a directory");
            }
            return Ok(PathBuf::from(dir));
        }
        i += 1;
    }
    if let Some(dir) = env.filter(|s| !s.is_empty()) {
        return Ok(PathBuf::from(dir));
    }
    Ok(exe_dir.join("Profile").join("User"))
}

/// Resolve the profile root from the current process arguments and executable.
pub fn profile_dir() -> Result<PathBuf> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let env = std::env::var(PROFILE_ENV)
        .ok()
        .or_else(|| std::env::var(LEGACY_PROFILE_ENV).ok());
    let exe = std::env::current_exe().context("cannot locate own executable")?;
    let dir = exe.parent().context("executable has no parent directory")?;
    resolve_profile_dir(&args, env.as_deref(), dir)
}

/// `<profile>/<name>`; the caller is responsible for creating the directory.
pub fn profile_file(name: &str) -> Result<PathBuf> {
    Ok(profile_dir()?.join(name))
}

/// Create the profile directory, import a legacy exe-adjacent file once, then
/// load (or create) the profile file.
pub fn load_in_profile(
    profile: &Path,
    legacy_dir: &Path,
    name: &str,
    default_toml: &str,
) -> Result<(PathBuf, String)> {
    std::fs::create_dir_all(profile)
        .with_context(|| format!("cannot create {}", profile.display()))?;
    let path = profile.join(name);
    import_legacy_once(&path, legacy_dir, &[name])?;
    let text = load_or_create(&path, default_toml)?;
    Ok((path, text))
}

/// The app-facing entry point: resolve the profile, import a legacy file once,
/// and return both the path (for Edit Config/persist) and the TOML text.
pub fn load_profile(name: &str, default_toml: &str) -> Result<(PathBuf, String)> {
    let profile = profile_dir()?;
    let exe = std::env::current_exe().context("cannot locate own executable")?;
    let legacy = exe.parent().context("executable has no parent directory")?;
    load_in_profile(&profile, legacy, name, default_toml)
}

/// Load a profile file, importing the first available legacy file name once.
pub fn load_profile_with_legacy(
    name: &str,
    legacy_names: &[&str],
    default_toml: &str,
) -> Result<(PathBuf, String)> {
    let profile = profile_dir()?;
    let exe = std::env::current_exe().context("cannot locate own executable")?;
    let legacy = exe.parent().context("executable has no parent directory")?;
    std::fs::create_dir_all(&profile)
        .with_context(|| format!("cannot create {}", profile.display()))?;
    let path = profile.join(name);
    import_legacy_once(&path, legacy, legacy_names)?;
    let text = load_or_create(&path, default_toml)?;
    Ok((path, text))
}

/// Copy a legacy sibling to a new path without overwriting an existing file.
pub fn import_legacy_once(path: &Path, legacy_dir: &Path, legacy_names: &[&str]) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    let parent = path
        .parent()
        .context("config path has no parent directory")?;
    std::fs::create_dir_all(parent)
        .with_context(|| format!("cannot create {}", parent.display()))?;
    for name in legacy_names {
        let legacy = legacy_dir.join(name);
        if legacy.is_file() {
            std::fs::copy(&legacy, path)
                .with_context(|| format!("cannot migrate {}", legacy.display()))?;
            eprintln!("migrated {} -> {}", legacy.display(), path.display());
            break;
        }
    }
    Ok(())
}

/// `path` if it exists, else the default TOML written to `path` first.
///
/// `default_toml` is embedded by the app with `include_str!("../appname.toml")`.
pub fn load_or_create(path: &Path, default_toml: &str) -> Result<String> {
    if !path.exists() {
        std::fs::write(path, default_toml)
            .with_context(|| format!("cannot create default {}", path.display()))?;
    }
    std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))
}

pub fn modified(path: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Spawn a fresh copy with the same profile override, then let the caller quit.
pub fn restart_self() {
    let Ok(me) = std::env::current_exe() else {
        return;
    };
    let mut cmd = std::process::Command::new(me);
    if let Ok(profile) = profile_dir() {
        cmd.arg("--profile").arg(profile);
    }
    // The new copy waits for this PID to exit before taking the single-instance
    // mutex, otherwise Reload races into "already running".
    cmd.arg("--wait-pid").arg(std::process::id().to_string());
    let _ = cmd.spawn();
}

/// `<exe_dir>/<name>` — legacy path, used as the one-time import source.
pub fn exe_dir_file(name: &str) -> Result<PathBuf> {
    let exe = std::env::current_exe().context("cannot locate own executable")?;
    let dir = exe.parent().context("executable has no parent directory")?;
    Ok(dir.join(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kit-profile-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn profile_resolution_order() {
        let exe = PathBuf::from("C:/app");
        assert_eq!(
            resolve_profile_dir(&[], None, &exe).unwrap(),
            exe.join("Profile").join("User")
        );

        let args = vec!["--profile".to_string(), "D:/cfg".to_string()];
        assert_eq!(
            resolve_profile_dir(&args, Some("E:/env"), &exe).unwrap(),
            PathBuf::from("D:/cfg")
        );

        let args = vec!["--profile=D:/cfg2".to_string()];
        assert_eq!(
            resolve_profile_dir(&args, None, &exe).unwrap(),
            PathBuf::from("D:/cfg2")
        );

        assert_eq!(
            resolve_profile_dir(&[], Some("E:/env"), &exe).unwrap(),
            PathBuf::from("E:/env")
        );

        assert!(resolve_profile_dir(&["--profile".to_string()], None, &exe).is_err());
        assert!(resolve_profile_dir(&["--profile=".to_string()], None, &exe).is_err());
    }

    #[test]
    fn first_run_creates_the_default() {
        let root = temp_dir("default");
        let profile = root.join("Profile").join("User");
        let legacy = root.join("legacy");
        std::fs::create_dir_all(&legacy).unwrap();

        let (path, text) = load_in_profile(&profile, &legacy, "app.toml", "answer = 42\n").unwrap();
        assert_eq!(text, "answer = 42\n");
        assert!(path.exists());
        assert_eq!(path, profile.join("app.toml"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn legacy_is_imported_once_and_never_overwrites() {
        let root = temp_dir("migrate");
        let profile = root.join("Profile").join("User");
        let legacy = root.join("legacy");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join("app.toml"), "old = 1\n").unwrap();

        let (path, text) = load_in_profile(&profile, &legacy, "app.toml", "new = 1\n").unwrap();
        assert_eq!(text, "old = 1\n");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "old = 1\n");

        // The profile now wins; later edits to the legacy file are ignored.
        std::fs::write(legacy.join("app.toml"), "old = 2\n").unwrap();
        std::fs::write(&path, "user = 3\n").unwrap();
        let (_, text) = load_in_profile(&profile, &legacy, "app.toml", "new = 1\n").unwrap();
        assert_eq!(text, "user = 3\n");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn renamed_legacy_file_is_imported_without_overwriting() {
        let root = temp_dir("renamed");
        let legacy = root.join("legacy");
        let profile = root.join("profile");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join("desktop-tools.toml"), "old = 1\n").unwrap();

        let path = profile.join("window-tools.toml");
        import_legacy_once(&path, &legacy, &["desktop-tools.toml", "window-tools.toml"]).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "old = 1\n");

        std::fs::write(legacy.join("desktop-tools.toml"), "old = 2\n").unwrap();
        import_legacy_once(&path, &legacy, &["desktop-tools.toml"]).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "old = 1\n");
        let _ = std::fs::remove_dir_all(&root);
    }
}
