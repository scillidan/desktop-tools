//! `config/core.toml`: host, suite editor and plugin enable settings.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct CoreConfig {
    #[serde(default)]
    pub host: HostSection,
    #[serde(default)]
    pub editor: EditorSection,
    #[serde(default)]
    pub plugins: PluginsSection,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct EditorSection {
    #[serde(default)]
    pub command: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct HostSection {
    #[serde(default)]
    pub start_with_windows: bool,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default = "default_tray")]
    pub tray: String,
}

impl Default for HostSection {
    fn default() -> Self {
        Self {
            start_with_windows: false,
            log_level: default_log_level(),
            tray: default_tray(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct PluginsSection {
    #[serde(default)]
    pub enabled: Vec<String>,
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_tray() -> String {
    "host".to_string()
}

impl CoreConfig {
    pub fn parse(text: &str) -> Result<CoreConfig> {
        toml::from_str(text).context("invalid core.toml")
    }

    pub fn enabled_set(&self) -> HashSet<String> {
        self.plugins.enabled.iter().cloned().collect()
    }

    pub fn set_enabled(&mut self, id: &str, enabled: bool) {
        let already = self.plugins.enabled.iter().any(|x| x == id);
        if enabled && !already {
            self.plugins.enabled.push(id.to_string());
            self.plugins.enabled.sort();
            self.plugins.enabled.dedup();
        } else if !enabled {
            self.plugins.enabled.retain(|x| x != id);
        }
    }

    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(self).context("cannot serialize core.toml")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_dedupes_enabled_ids() {
        let core = CoreConfig::parse(
            r#"
[host]
start_with_windows = false
log_level = "debug"
tray = "host"

[plugins]
enabled = ["b", "a", "b"]
"#,
        )
        .unwrap();
        assert!(!core.host.start_with_windows);
        assert_eq!(core.host.log_level, "debug");
        let set = core.enabled_set();
        assert!(set.contains("a"));
        assert!(set.contains("b"));
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn missing_sections_use_defaults() {
        let core = CoreConfig::parse("").unwrap();
        assert!(core.host.tray == "host");
        assert!(core.plugins.enabled.is_empty());
    }

    #[test]
    fn plugin_enable_changes_preserve_the_suite_editor() {
        let mut core = CoreConfig::parse(
            "[editor]\ncommand = 'code --new-window'\n[plugins]\nenabled = []\n",
        )
        .unwrap();
        core.set_enabled("window-tools", true);
        let text = core.to_toml().unwrap();
        assert_eq!(core.editor.command, "code --new-window");
        assert_eq!(
            CoreConfig::parse(&text).unwrap().editor.command,
            "code --new-window"
        );
    }

    #[test]
    fn toggle_round_trips_through_toml() {
        let mut core = CoreConfig::default();
        core.set_enabled("a", true);
        core.set_enabled("b", true);
        core.set_enabled("a", false);
        let text = core.to_toml().unwrap();
        let again = CoreConfig::parse(&text).unwrap();
        assert_eq!(again.plugins.enabled, vec!["b"]);
    }
}
