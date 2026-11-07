# desktop-tools

Windows desktop tools, merged into one Cargo workspace. Functional domains are
**window / screen / keyboard / taskbar**. A domain is a code, configuration and
menu boundary, not necessarily a separate process. Native tools appear before
external integrations in documentation, code registration and tray menus.

## Status

- Merge P1 (host readiness in `keyboard-tools-rs`) is signed off.
- P2 is signed off, including hosted and standalone checks.
- P3 window import and the approved module/menu split have 125 passing tests
  (plus eight intentionally ignored real-desktop tests), clean clippy/release,
  six-plugin smoke and hosted Suspend/Resume/Reload/Quit checks. Physical
  sign-off is pending. Screen, taskbar, KeyMap and application toggles are not imported yet.

[`MERGE.md`](MERGE.md) records architecture, source scope and physical gates.
[`CHECKLIST.md`](CHECKLIST.md) records the current build and handoff.

## Current plugins

Native window/keyboard tools first, then external GoldenDict integrations:

1. `window-tools`
2. `keyboard-auto-switch-back`
3. `keyboard-locker`
4. `keyboard-switch`
5. `goldendict-auto-search` (external)
6. `goldendict-search-in-group` (external)

`window-tools` is one hosted process and one standalone executable, but its
Window menu and code are divided into native modules:

- `window-rectangle`: geometry, snap/resize/pin/minimize state, undo and mouse drag;
- `window-switcher`: same-application window cycling;
- `window-quick-switch`: file-dialog folder tracking and navigation;
- `window-vdesk`: native virtual desktops;
- `window-core`: internal domain primitives and narrow query interfaces;
- `window-tools`: composition root, hooks, configuration, CLI, host IPC and tray.

The modules share suspend/reload and do not add independent processes or
per-module Enable/Restart switches.

Each plugin still has its own `plugin.toml` and config file. `group` controls
menu ownership; `external = true` places an integration after native tools in
the same group. Manifest API remains 1. Newline-JSON wire protocol v3 adds nested
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
