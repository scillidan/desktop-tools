//! Discover `plugins/<id>/plugin.toml`; invalid packages are reported and
//! skipped, never allowed to stop the host.

use crate::manifest::{group_index, Manifest, GROUPS};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Plugin {
    pub manifest: Manifest,
    pub dir: PathBuf,
}

#[derive(Debug, Default)]
pub struct Catalog {
    pub plugins: Vec<Plugin>,
    pub errors: Vec<String>,
}

impl Catalog {
    pub fn discover(root: &Path) -> Catalog {
        let mut catalog = Catalog::default();
        let entries = match std::fs::read_dir(root) {
            Ok(entries) => entries,
            Err(e) => {
                catalog
                    .errors
                    .push(format!("{}: cannot read: {e}", root.display()));
                return catalog;
            }
        };
        for entry in entries.flatten() {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            let manifest_path = dir.join("plugin.toml");
            if !manifest_path.is_file() {
                continue;
            }
            match Manifest::load(&manifest_path) {
                Ok(manifest) => catalog.plugins.push(Plugin { manifest, dir }),
                Err(e) => catalog.errors.push(format!("{}: {e:#}", dir.display())),
            }
        }
        catalog.plugins.sort_by(|a, b| {
            group_index(&a.manifest.group)
                .unwrap_or(GROUPS.len())
                .cmp(&group_index(&b.manifest.group).unwrap_or(GROUPS.len()))
                .then_with(|| a.manifest.external.cmp(&b.manifest.external))
                .then_with(|| {
                    a.manifest
                        .name
                        .to_lowercase()
                        .cmp(&b.manifest.name.to_lowercase())
                })
                .then_with(|| a.manifest.id.cmp(&b.manifest.id))
        });
        catalog
    }

    pub fn get(&self, id: &str) -> Option<&Plugin> {
        self.plugins.iter().find(|p| p.manifest.id == id)
    }
}

/// Filter enabled ids against the catalog. Unknown ids are returned so the
/// host can log them once.
pub fn enabled_plugins<'a>(
    catalog: &'a Catalog,
    enabled: &HashSet<String>,
) -> (Vec<&'a Plugin>, Vec<String>) {
    let mut active = Vec::new();
    let mut unknown = Vec::new();
    for id in enabled {
        match catalog.get(id) {
            Some(plugin) => active.push(plugin),
            None => unknown.push(id.clone()),
        }
    }
    active.sort_by(|a, b| {
        group_index(&a.manifest.group)
            .unwrap_or(GROUPS.len())
            .cmp(&group_index(&b.manifest.group).unwrap_or(GROUPS.len()))
            .then_with(|| a.manifest.external.cmp(&b.manifest.external))
            .then_with(|| {
                a.manifest
                    .name
                    .to_lowercase()
                    .cmp(&b.manifest.name.to_lowercase())
            })
            .then_with(|| a.manifest.id.cmp(&b.manifest.id))
    });
    unknown.sort();
    (active, unknown)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("plugin-catalog-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn make(root: &Path, id: &str, api: u32) {
        make_kind(root, id, api, false, "keyboard");
    }

    fn make_group(root: &Path, id: &str, group: &str) {
        make_kind(root, id, 1, false, group);
    }

    fn make_kind(root: &Path, id: &str, api: u32, external: bool, group: &str) {
        let dir = root.join(id);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{id}.exe")), b"x").unwrap();
        std::fs::write(
            dir.join("plugin.toml"),
            format!(
                "id = \"{id}\"\nname = \"{id}\"\nversion = \"0.1\"\napi = {api}\nexe = \"{id}.exe\"\ngroup = \"{group}\"\nexternal = {external}\n"
            ),
        )
        .unwrap();
    }

    #[test]
    fn discovery_skips_invalid_and_sorts() {
        let root = temp_root("discover");
        make(&root, "b-tool", 1);
        make(&root, "a-tool", 1);
        make(&root, "bad-api", 2);
        make_kind(&root, "a-external", 1, true, "keyboard");
        make_group(&root, "a-window", "window");
        let catalog = Catalog::discover(&root);
        assert_eq!(catalog.plugins.len(), 4);
        assert_eq!(catalog.plugins[0].manifest.id, "a-window");
        assert_eq!(catalog.plugins[1].manifest.id, "a-tool");
        assert_eq!(catalog.plugins[2].manifest.id, "b-tool");
        assert_eq!(catalog.plugins[3].manifest.id, "a-external");
        assert_eq!(catalog.errors.len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn native_plugins_sort_by_display_name_before_external_tools() {
        let root = temp_root("display-order");
        for (id, name, external) in [
            ("z-native", "Alpha", false),
            ("a-native", "Zulu", false),
            ("a-external", "A External", true),
        ] {
            let dir = root.join(id);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join(format!("{id}.exe")), b"x").unwrap();
            std::fs::write(
                dir.join("plugin.toml"),
                format!("id = \"{id}\"\nname = \"{name}\"\nversion = \"0.1\"\napi = 1\nexe = \"{id}.exe\"\ngroup = \"keyboard\"\nexternal = {external}\n"),
            )
            .unwrap();
        }
        let catalog = Catalog::discover(&root);
        let names: Vec<&str> = catalog
            .plugins
            .iter()
            .map(|plugin| plugin.manifest.name.as_str())
            .collect();
        assert_eq!(names, ["Alpha", "Zulu", "A External"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn enabled_filter_reports_unknown_ids() {
        let root = temp_root("enabled");
        make(&root, "a-tool", 1);
        let catalog = Catalog::discover(&root);
        let enabled: HashSet<String> = ["a-tool", "missing-tool"]
            .into_iter()
            .map(str::to_string)
            .collect();
        let (active, unknown) = enabled_plugins(&catalog, &enabled);
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].manifest.id, "a-tool");
        assert_eq!(unknown, vec!["missing-tool"]);
        let _ = std::fs::remove_dir_all(&root);
    }
}
