//! TOML config: profile resolution, first-run creation, one-time legacy import.
//!
//! The profile root is `--profile <dir>` > `DTTOOLS_PROFILE` >
//! legacy `KBTOOLS_PROFILE` > an existing `<exe_dir>/config` (or legacy
//! `<exe_dir>/Profile/User`) > an existing `%APPDATA%/desktop-tools/config`
//! (or its legacy `Profile/User`) > the new standard `config` path.
//! The exe-adjacent `config` path is also the fallback when `APPDATA` is unavailable.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

pub const PROFILE_ENV: &str = "DTTOOLS_PROFILE";
pub const LEGACY_PROFILE_ENV: &str = "KBTOOLS_PROFILE";

/// Resolve the profile root. Existing portable and legacy directories are
/// preferred over the fresh-install default; no directory is created here.
pub fn resolve_profile_dir(
    args: &[String],
    env: Option<&str>,
    exe_dir: &Path,
    app_data: Option<&Path>,
) -> Result<PathBuf> {
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--profile" {
            let dir = args.get(i + 1).context("--profile requires a directory")?;
            if dir.is_empty() {
                bail!("--profile requires a directory");
            }
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
    let portable = exe_dir.join("config");
    let legacy_portable = exe_dir.join("Profile").join("User");
    if portable.is_dir() || legacy_portable.is_dir() {
        return Ok(if portable.is_dir() {
            portable
        } else {
            legacy_portable
        });
    }
    if let Some(app_data) = app_data.filter(|path| !path.as_os_str().is_empty()) {
        let root = app_data.join("desktop-tools");
        let standard = root.join("config");
        let legacy = root.join("Profile").join("User");
        if standard.is_dir() || !legacy.is_dir() {
            return Ok(standard);
        }
        return Ok(legacy);
    }
    Ok(portable)
}

/// Resolve the profile root from the current process arguments and executable.
pub fn profile_dir() -> Result<PathBuf> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let env = std::env::var(PROFILE_ENV)
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| {
            std::env::var(LEGACY_PROFILE_ENV)
                .ok()
                .filter(|value| !value.is_empty())
        });
    let app_data = std::env::var_os("APPDATA").map(PathBuf::from);
    let exe = std::env::current_exe().context("cannot locate own executable")?;
    let dir = exe.parent().context("executable has no parent directory")?;
    let profile = resolve_profile_dir(&args, env.as_deref(), dir, app_data.as_deref())?;
    std::path::absolute(profile).context("cannot resolve an absolute profile directory")
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

/// Merge sparse user TOML over embedded defaults without modifying either file.
/// Tables merge recursively; every other value, including arrays, is replaced.
pub fn merge_toml(default_toml: &str, user_toml: &str) -> Result<String> {
    fn merge(base: &mut toml::Value, user: toml::Value) {
        match (base, user) {
            (toml::Value::Table(base), toml::Value::Table(user)) => {
                for (key, value) in user {
                    match base.get_mut(&key) {
                        Some(current) => merge(current, value),
                        None => {
                            base.insert(key, value);
                        }
                    }
                }
            }
            (base, user) => *base = user,
        }
    }

    let mut base: toml::Value = toml::from_str(default_toml).context("invalid embedded default")?;
    let user: toml::Value = toml::from_str(user_toml).context("invalid user config")?;
    merge(&mut base, user);
    toml::to_string_pretty(&base).context("cannot serialize merged config")
}

/// Load sparse user overrides and keep a generated default reference beside them.
pub fn load_layered_in_profile(
    profile: &Path,
    legacy_dir: &Path,
    name: &str,
    default_toml: &str,
) -> Result<(PathBuf, String)> {
    std::fs::create_dir_all(profile)
        .with_context(|| format!("cannot create {}", profile.display()))?;
    let path = profile.join(name);
    import_legacy_once(&path, legacy_dir, &[name])?;

    let reference = profile.join("defaults").join(name);
    std::fs::create_dir_all(
        reference
            .parent()
            .context("default reference has no parent")?,
    )?;
    if std::fs::read_to_string(&reference).ok().as_deref() != Some(default_toml) {
        std::fs::write(&reference, default_toml)
            .with_context(|| format!("cannot write {}", reference.display()))?;
    }
    if !path.exists() {
        std::fs::write(
            &path,
            format!(
                "# User overrides for {name}; omitted settings inherit defaults/{name}.\n\
                 # Tables merge recursively. Arrays replace defaults, including an empty [].\n\
                 # defaults/{name} is a generated reference; edit this file instead.\n"
            ),
        )
        .with_context(|| format!("cannot create overrides {}", path.display()))?;
    }
    let user = std::fs::read_to_string(&path)
        .with_context(|| format!("cannot read {}", path.display()))?;
    Ok((path, merge_toml(default_toml, &user)?))
}

/// Resolve the profile and load sparse overrides over an embedded default.
pub fn load_layered_profile(name: &str, default_toml: &str) -> Result<(PathBuf, String)> {
    let profile = profile_dir()?;
    let exe = std::env::current_exe().context("cannot locate own executable")?;
    let legacy = exe.parent().context("executable has no parent directory")?;
    load_layered_in_profile(&profile, legacy, name, default_toml)
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

/// Expand `%NAME%` variables while preserving unknown or incomplete variables.
pub fn expand_env(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let tail = &rest[start + 1..];
        match tail.find('%') {
            Some(end) => {
                let name = &tail[..end];
                match std::env::var(name) {
                    Ok(value) if !name.is_empty() => out.push_str(&value),
                    _ => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &tail[end + 1..];
            }
            None => {
                out.push('%');
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    out
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
        let root = temp_dir("resolution");
        let exe = root.join("app");
        let app_data = root.join("roaming");
        let portable = exe.join("config");
        let legacy_portable = exe.join("Profile").join("User");
        let standard = app_data.join("desktop-tools").join("config");
        let legacy_standard = app_data.join("desktop-tools").join("Profile").join("User");
        std::fs::create_dir_all(&exe).unwrap();

        assert_eq!(
            resolve_profile_dir(&[], None, &exe, Some(&app_data)).unwrap(),
            standard
        );
        // Without APPDATA, retain the exe-adjacent fallback using the new name.
        assert_eq!(
            resolve_profile_dir(&[], None, &exe, None).unwrap(),
            portable
        );

        std::fs::create_dir_all(&legacy_portable).unwrap();
        assert_eq!(
            resolve_profile_dir(&[], None, &exe, Some(&app_data)).unwrap(),
            legacy_portable
        );
        std::fs::create_dir_all(&portable).unwrap();
        assert_eq!(
            resolve_profile_dir(&[], None, &exe, Some(&app_data)).unwrap(),
            portable
        );
        assert_eq!(
            resolve_profile_dir(&[], Some("E:/env"), &exe, Some(&app_data)).unwrap(),
            PathBuf::from("E:/env")
        );

        let standard_exe = root.join("standard-app");
        std::fs::create_dir_all(&standard_exe).unwrap();
        std::fs::create_dir_all(&legacy_standard).unwrap();
        assert_eq!(
            resolve_profile_dir(&[], None, &standard_exe, Some(&app_data)).unwrap(),
            legacy_standard
        );
        std::fs::create_dir_all(&standard).unwrap();
        assert_eq!(
            resolve_profile_dir(&[], None, &standard_exe, Some(&app_data)).unwrap(),
            standard
        );

        let args = vec!["--profile".to_string(), "D:/cfg".to_string()];
        assert_eq!(
            resolve_profile_dir(&args, Some("E:/env"), &exe, Some(&app_data)).unwrap(),
            PathBuf::from("D:/cfg")
        );
        let args = vec!["--profile=D:/cfg2".to_string()];
        assert_eq!(
            resolve_profile_dir(&args, None, &exe, Some(&app_data)).unwrap(),
            PathBuf::from("D:/cfg2")
        );

        assert!(resolve_profile_dir(&["--profile".to_string()], None, &exe, None).is_err());
        assert!(resolve_profile_dir(&["--profile=".to_string()], None, &exe, None).is_err());
        assert!(
            resolve_profile_dir(&["--profile".to_string(), String::new()], None, &exe, None)
                .is_err()
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sparse_overlay_merges_tables_and_replaces_arrays() {
        let defaults = r#"
            title = "default"
            [settings]
            mode = "InAll"
            hotkey = "capslock"
            [[layout]]
            klid = "00000409"
            [[layout]]
            klid = "00000804"
        "#;
        let user = r#"
            layout = []
            [settings]
            hotkey = "ctrl+alt+k"
        "#;
        let merged: toml::Value = toml::from_str(&merge_toml(defaults, user).unwrap()).unwrap();
        assert_eq!(merged["title"].as_str(), Some("default"));
        assert_eq!(merged["settings"]["mode"].as_str(), Some("InAll"));
        assert_eq!(merged["settings"]["hotkey"].as_str(), Some("ctrl+alt+k"));
        assert_eq!(merged["layout"].as_array().unwrap().len(), 0);

        let sparse: toml::Value = toml::from_str(&merge_toml(defaults, "").unwrap()).unwrap();
        assert_eq!(sparse["layout"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn layered_profile_keeps_user_file_sparse_and_reference_current() {
        let root = temp_dir("layered");
        let profile = root.join("config");
        let legacy = root.join("legacy");
        std::fs::create_dir_all(&legacy).unwrap();
        let (path, text) = load_layered_in_profile(
            &profile,
            &legacy,
            "app.toml",
            "[settings]\nanswer = 42\nother = true\n",
        )
        .unwrap();
        assert!(std::fs::read_to_string(&path).unwrap().contains("override"));
        let merged: toml::Value = toml::from_str(&text).unwrap();
        assert_eq!(merged["settings"]["answer"].as_integer(), Some(42));
        assert_eq!(
            std::fs::read_to_string(profile.join("defaults").join("app.toml")).unwrap(),
            "[settings]\nanswer = 42\nother = true\n"
        );

        std::fs::write(&path, "[settings]\nanswer = 7\n").unwrap();
        let (_, text) = load_layered_in_profile(
            &profile,
            &legacy,
            "app.toml",
            "[settings]\nanswer = 43\nother = false\n",
        )
        .unwrap();
        let merged: toml::Value = toml::from_str(&text).unwrap();
        assert_eq!(merged["settings"]["answer"].as_integer(), Some(7));
        assert_eq!(merged["settings"]["other"].as_bool(), Some(false));
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "[settings]\nanswer = 7\n"
        );
        let _ = std::fs::remove_dir_all(&root);
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
