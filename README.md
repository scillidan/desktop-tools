# desktop-tools

Windows desktop tools, merged into one Cargo workspace. Functional domains are
**window / screen / keyboard / taskbar**. A domain is a code, configuration and
menu boundary, not necessarily a separate process. Native tools appear before
external integrations in documentation, code registration and tray menus.

## Status

- Merge P1 (host readiness in `keyboard-tools-rs`) is signed off.
- P2 is signed off, including hosted and standalone checks.
- P3 window import, feature ownership and per-module configuration have clean
  fmt/tests/clippy/release/staging, six-plugin smoke and real wire-v3 checks for
  module reload, invalid-config rollback and lifecycle. Hosted and standalone
  physical passes are signed off. P4a moves all 67 legacy shell suppressions into
  the independent **Shell Keys** plugin (`keyboard-shell.toml`), with a real
  Suspend Shell Keys toggle. **Key Map** is a separate plugin with immediate
  static Unicode replacement and same-process automatic config reload. Native
  plugins and Window feature menus sort by display name; external tools follow.
  Screen and application toggles are not imported yet. P5 imports the native
  taskbar tools with profile/host IPC and clean worker restoration; its physical
  pass is pending.

[`MERGE.md`](MERGE.md) records architecture, source scope and physical gates.
[`CHECKLIST.md`](CHECKLIST.md) records the current build and handoff.

## Current plugins

Native window/keyboard/taskbar tools first, then external GoldenDict integrations:

1. `window-tools`
2. `taskbar-tools`
3. `keyboard-keymap`
4. `keyboard-auto-switch-back`
5. `keyboard-locker`
6. `keyboard-switch`
7. `keyboard-shell`
8. `goldendict-auto-search` (external)
9. `goldendict-search-in-group` (external)

`window-tools` is one hosted process and one standalone executable, but its
Window menu and code are divided into native modules:

- `window-rectangle`: snap/center/display geometry and state;
- `window-resize`: Sizer and resize presets;
- `window-mouse-drag`: modifier-drag state and mouse hook;
- `window-on-top-bottom`: topmost/bottommost pins and shared cleanup;
- `window-maximize-minimize`: maximize/restore/minimize;
- `window-app-switch`: same-application window cycling;
- `window-quick-switch`: file-dialog folder tracking and navigation;
- `window-virtual-desktops`: native virtual desktops;
- `window-core`: internal domain primitives and narrow query interfaces;
- `window-tools`: composition root, hooks, configuration, CLI, host IPC and tray.

The modules share suspend/reload/undo and do not add independent processes or
per-module Enable/Restart switches.

`taskbar-tools` retains its three worker-based features in `taskbar-center`,
`taskbar-hidden` and `taskbar-pin`, with automatic reload, group suspend and
restore-on-exit. Each feature has its own `taskbar-<feature>.toml`, submenu and
edit entry. `taskbar-core` holds shared Explorer and migration primitives.
Standalone retains `--dump`, `--inspect`, `--once`, its own tray and Start with
Windows; hosted errors use the host notification path.

Each Window module has its own schema, defaults, tests and
`window-<module>.toml` edit entry. Root-only actions such as Undo and the temporary
screen binding live in `window-actions.toml`. Window and Taskbar migrate an old
aggregate TOML once: existing feature files win, missing effective settings are
materialized, and the aggregate is renamed to a `.legacy.toml` backup. Runtime
and menus no longer use the aggregate. Window files share one hotkey namespace:
duplicates are rejected, and an invalid file leaves the complete previous Window
configuration active.

Each plugin still has its own `plugin.toml` and config file. `group` controls
menu ownership; `external = true` places an integration after native tools in
the same group. Nested plugin wrappers show process status in their labels;
inside, an optional manifest `description` and separator replace duplicate
status, version and restart counters. Single-plugin groups put the process
status on the group title itself, such as `Window (running)`, without adding a
duplicate plugin layer; multi-plugin groups retain per-plugin wrappers. Every
Window feature submenu also has a separator after its grey description.
Suspend temporarily pauses hooks without
stopping the process; Disable persistently stops the plugin and performs normal
cleanup. Window and Taskbar have no aggregate config entry; Window instead has
`Edit Window actions config`, while every feature entry edits its own TOML.
Manifest API remains 1. Newline-JSON wire protocol v3 adds nested
submenus; host and all plugins must come from the same staged package.

## Build and test

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\verify.ps1
```

The script enforces the repository-local `target` directory, then runs fmt,
workspace tests, clippy, a release build and staging. Close running instances
before staging so Windows can replace the executables.

Flat runtime layout:

```text
stage\
  desktop-tools.exe
  plugins\
    window-tools\...
    taskbar-tools\...
    keyboard-keymap\...
    keyboard-shell\...
    keyboard-locker\...
    keyboard-switch\...
    keyboard-auto-switch-back\...
    goldendict-search-in-group\...
    goldendict-auto-search\...
```

Run `stage\desktop-tools.exe`, optionally with `--profile <dir>`. Profile
priority is `--profile` > `DTTOOLS_PROFILE` > legacy `KBTOOLS_PROFILE` >
`Profile\User` beside the executable. Existing keyboard profiles therefore work
without copying. Enabling the new host's Windows autostart removes the legacy
`keyboard-tools-rs` Run entry; standalone plugin autostart names are unchanged.

`core.toml` owns the enabled plugin ids. Restarting a disabled plugin enables
it, persists that state and shows `Disable <id>` consistently.
