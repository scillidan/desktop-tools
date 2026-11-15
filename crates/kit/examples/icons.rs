//! Regenerate all committed executable/state icons after editing kit::icon.

use kit::icon::{self, State};
use std::path::{Path, PathBuf};

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for &(exe, spec) in icon::PRODUCTS {
        let dir = asset_dir(&root, exe);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{exe}.ico"));
        std::fs::write(&path, icon::ico(spec, State::Normal)).unwrap();
        println!("{}", path.display());
    }
    let screen = asset_dir(&root, "screen-tools");
    for (name, state) in [
        ("suspended", State::Suspended),
        ("error", State::Error),
        ("disabled", State::Disabled),
    ] {
        let path = screen.join(format!("screen-tools.{name}.ico"));
        std::fs::write(&path, icon::ico(icon::SCREEN, state)).unwrap();
        println!("{}", path.display());
    }
}

fn asset_dir(root: &Path, exe: &str) -> PathBuf {
    let crate_dir = match exe {
        "desktop-tools" => "apps/desktop-tools",
        "window-tools" => "domains/window/window-tools",
        "screen-tools" => "domains/screen/screen-tools",
        "taskbar-tools" => "domains/taskbar/taskbar-tools",
        _ => match exe {
            "keyboard-switch" => "plugins/keyboard-switch",
            "keyboard-shell" => "plugins/keyboard-shell",
            "keyboard-keymap" => "plugins/keyboard-keymap",
            "keyboard-locker" => "plugins/keyboard-locker",
            "keyboard-auto-switch-back" => "plugins/keyboard-auto-switch-back",
            "goldendict-search-in-group" => "plugins/goldendict-search-in-group",
            "goldendict-auto-search" => "plugins/goldendict-auto-search",
            _ => unreachable!("PRODUCTS and asset_dir must stay in sync"),
        },
    };
    root.join(crate_dir).join("assets")
}
