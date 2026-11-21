# desktop-tools

Windows desktop tools, merged into one Cargo workspace. Functional domains are **window / screen / keyboard / taskbar**. A domain is a code, configuration and menu boundary, not necessarily a separate process. Native tools appear before external integrations in documentation, code registration and tray menus.

Authors: Kimi-K3.1-Pro🧙‍♂️, scillidan🤡

## Status

- Merge P1 (host readiness in `keyboard-tools-rs`) is signed off.
- P2 is signed off, including hosted and standalone checks.
- P1-P5 are signed off, including hosted/standalone lifecycle and physical checks. P4 splits **Shell Keys** (66 suppressions) from **Key Map** (immediate static Unicode replacement). P6a renamed the screen source to **Screen Tools** and passed standalone draw, capture/OCR and recording acceptance. P6b imports Screen into the shared profile and host with clean wire-v3 Suspend/Resume/Quit, ten-plugin smoke and exit-code verification. Hosted Screen hosted and standalone physical acceptance are complete, including feature crates and split configuration. P6c Power, Scale and Flameshot handoff are implemented and physically signed. P7 App Toggle and P8 two-window swapping are also signed off, including their default hotkeys. P9a shared executable and tray icons passed hosted and standalone visual acceptance.

[`MERGE.md`](MERGE.md) records architecture, source scope and physical gates. [`CHECKLIST.md`](CHECKLIST.md) records the current build and handoff.

## Current plugins

Native window/screen/keyboard/taskbar tools first, then external GoldenDict integrations:

1. `window-tools`
2. `screen-tools`
3. `taskbar-tools`
4. `keyboard-keymap`
5. `keyboard-auto-switch-back`
6. `keyboard-locker`
7. `keyboard-switch`
8. `keyboard-shell`
9. `goldendict-auto-search` (external)
10. `goldendict-search-in-group` (external)

`window-tools` is one hosted process and one standalone executable, but its Window menu and code are divided into native modules:

- `window-rectangle`: snap/center/display geometry, two-window `swapWindows` and state;
- `window-resize`: Sizer, resize presets and DWM window zoom (`window-scale.toml`);
- `window-mouse-drag`: modifier-drag state and mouse hook;
- `window-on-top-bottom`: topmost/bottommost pins and shared cleanup;
- `window-maximize-minimize`: maximize/restore/minimize;
- `window-app-switch`: same-application window cycling;
- `window-quick-switch`: file-dialog folder tracking and navigation;
- `window-virtual-desktops`: native virtual desktops;
- `window-app-toggle` (external, after native modules): focus or launch the exact
  configured Double Commander/WezTerm installation via `window-app-toggle.toml`;
- `window-core`: internal domain primitives and narrow query interfaces;
- `window-tools`: composition root, hooks, configuration, CLI, host IPC and tray.

The modules share suspend/reload/undo and do not add independent processes or per-module Enable/Restart switches. App Toggle matches canonical full executable paths (including Scoop `current` aliases), restores minimized targets and returns to the previous window before minimizing the current target. WezTerm selects the topmost existing window rather than cycling. Its ordinary hotkey uses no-repeat registration; `Win+E` uses the low-level hook to suppress Explorer and auto-repeat. Shell Keys retains its other 66 default suppressions and no longer owns `Win+E`. Rectangle's `swapWindows` action selects two eligible windows, exchanges their complete visible/outer rectangles and restores both with one Undo. Bare Esc or 15 seconds cancels the armed selection; Reload/Suspend/Quit also cancel it. The approved default is `Ctrl+Win+Alt+X`; custom bindings belong in `window-rectangle.toml`. Existing configuration files are not rewritten by upgrades.

Quick Switch watches the latest file-manager folder but navigates a file dialog only once per activation. Success or failure does not trigger another automatic attempt until focus leaves and returns to that dialog; the manual `fileDialog` binding remains available. Shell Keys documents the Windows function of every default suppression inline in `keyboard-shell.toml` without changing the binding set.

`screen-tools` retains the frame, Draw, Capture, OCR, Record and Flameshot handoff behavior from Screen Tools. Hosted mode has no second tray or legacy Quit hotkey and uses the original Win32 loop with a custom IPC wakeup. Suspend releases Screen hotkeys without changing Window/Keyboard/Taskbar. During Flameshot handoff, Screen releases its global hotkeys except Power so the external editor receives its own shortcuts, and reuses an existing Flameshot daemon. Hosted notifications use the host; standalone retains its own tray. Recording and audio live in `screen-record`; monitor power and window recovery live in `screen-power`; pixel capture, clipboard and PNG operations live in `screen-capture`; recognition, text layout, external engines and automatic OCR live in `screen-ocr`; frames, selection, OCR masks, pins and virtual-desktop frame coordination live in `screen-draw`; external annotation process/session handling lives in `flameshot-handoff`, with Frame visibility callbacks kept in Draw. Extraction preserves one process and coordinated capture/overlay/recording lifecycle.

Screen has alphabetically ordered native **Capture / Draw / OCR / Power / Record** submenus, followed by the external **Flameshot Handoff** entry. Each has a grey description, separator and its own edit entry. Their files are `screen-capture.toml`, `screen-draw.toml`, `screen-ocr.toml`, `screen-power.toml`, `screen-record.toml` and `screen-flameshot.toml`. The transitional `screen-actions.toml` is no longer read at runtime or shown in the menu; it remains an untouched one-time import source. Scale moved to `window-scale.toml` without rewriting `window-resize.toml` or the old source. The old aggregate `screen-tools.toml` migrates once to these files and is retained as `screen-tools.legacy.toml`; existing feature files are not overwritten. All files share one hotkey namespace and reload atomically.

`goldendict-search-in-group` supports multi-step hotkeys such as `ctrl+alt+shift+z > ctrl+alt+shift+z` in a group binding. Kit registers each unique combo once and matches up to 400 ms between steps; every step requires Ctrl, Alt or Win, auto-repeat does not advance the sequence, and ambiguous prefixes are rejected. Legacy doubled-character bindings such as `!xx` continue to use the same matcher. Existing profile bindings are not rewritten.

taskbar-tools` retains its three worker-based features in `taskbar-center`, `taskbar-hidden` and `taskbar-pin`, with automatic reload, group suspend and restore-on-exit. Each feature has its own `taskbar-<feature>.toml`, submenu and edit entry. `taskbar-core` holds shared Explorer and migration primitives. Standalone retains `--dump`, `--inspect`, `--once`, its own tray and Start with Windows; hosted errors use the host notification path.

Each Window module has its own schema, defaults, tests and `window-<module>.toml` edit entry. Root-only actions such as Undo live in `window-actions.toml`. Window, Screen and Taskbar migrate an old aggregate TOML once: existing feature files win, missing effective settings are materialized, and the aggregate is renamed to a `.legacy.toml` backup. Runtime and menus no longer use the aggregate. Each domain's files share one hotkey namespace: duplicates are rejected, and an invalid file leaves the complete previous domain configuration active.

Each plugin still has its own `plugin.toml` and config file. `group` controls menu ownership; `external = true` places an integration after native tools in the same group. Nested plugin wrappers show process status in their labels; inside, an optional manifest `description` and separator replace duplicate status, version and restart counters. Single-plugin groups put the process status on the group title itself, such as `Window (running)`, without adding a duplicate plugin layer; multi-plugin groups retain per-plugin wrappers. Every Window feature submenu also has a separator after its grey description. Suspend temporarily pauses hooks without stopping the process; Disable persistently stops the plugin and performs normal cleanup. Window, Screen and Taskbar have no aggregate config entry; Window instead has a root-only action config entry, while every feature entry edits its own TOML.

Manifest API remains 1. Newline-JSON wire protocol v3 adds nested submenus; host and all plugins must come from the same staged package.

## Icons

All 11 executables embed a shared rounded-keycap identity and use the same glyph for their standalone tray. Desktop is `D`, Window `W`, Screen `S`, Taskbar `T`; Keyboard and GoldenDict keep their function-specific letters and palettes. Suspended icons are grey, errors add a red badge, disabled/locked icons add a grey slash, and Window's transient reload success keeps its yellow badge. The host icon reflects plugin failure or an entirely disabled suite. A disabled standalone process has no tray icon.

`crates/kit/src/icon.rs` is the only renderer. Regenerate every checked-in asset and verify all 11 executable assets with:

```powershell
cargo run -p kit --example icons --target-dir target
cargo test -p kit every_executable_asset_matches_the_shared_renderer --target-dir target
```

Each ICO contains 16/20/24/32/48/256-pixel entries. `build.rs` embeds the normal icon in every exe; Screen additionally embeds Suspended/Error/Disabled resources for its native tray and Frame window class. Staging needs no icon sidecar files.

## Build and test

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\verify.ps1
```

The script enforces the repository-local `target` directory, then runs fmt, workspace tests, clippy, a release build and staging. Close running instances before staging so Windows can replace the executables.

Flat runtime layout:

```text
stage\
  desktop-tools.exe
  plugins\
    window-tools\...
    screen-tools\...
    taskbar-tools\...
    keyboard-keymap\...
    keyboard-shell\...
    keyboard-locker\...
    keyboard-switch\...
    keyboard-auto-switch-back\...
    goldendict-search-in-group\...
    goldendict-auto-search\...
```

Build one distribution archive with:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\package.ps1
```

The script runs the full verification and stage pipeline, refuses to package a running repo-local executable, and writes `dist/desktop-tools-<version>-win64.zip` plus its SHA-256 file. The archive contains no user profile.

## Install and external tools

Install from the `pile` Scoop bucket:

```powershell
scoop bucket add pile https://github.com/scillidan/scoop-pile
scoop install pile/desktop-tools
```

Or download `desktop-tools-<version>-win64.zip` from [Releases](https://github.com/scillidan/desktop-tools/releases), verify the adjacent SHA-256 file, extract it into a stable directory and run `desktop-tools.exe`. All ten plugins are enabled for a fresh profile. Use the tray's **Start with Windows** entry only after choosing the final install location.

The native window, taskbar, keyboard and Windows OCR features need no additional application. Recording needs `ffmpeg`; the optional RapidOCR and Tesseract engines need `uv` and `tesseract` respectively. GoldenDict integration and Flameshot handoff need those applications and their paths in the corresponding TOML files.

With Scoop, exit the host and use `scoop update desktop-tools` or `scoop uninstall desktop-tools`; its persisted profile is retained. For a manual zip upgrade, exit the host, replace the program files, and restart without deleting the profile. For manual uninstall, disable **Start with Windows**, exit the host, and remove the program directory; remove the profile separately only if its settings are no longer wanted.

Run `stage\desktop-tools.exe`, optionally with `--profile <dir>`. Profile priority is `--profile` > `DTTOOLS_PROFILE` > legacy `KBTOOLS_PROFILE` > an existing `Profile\User` beside the executable > `%APPDATA%\desktop-tools\Profile\User`. A relative explicit path is made absolute, and the host passes one absolute profile to every plugin. Release zips contain no profile, so ordinary manual installs use the Windows directory; creating `Profile\User` beside the exe opts into portable mode. Scoop persists that same portable directory. The top-level **Open Profile Folder** menu entry opens the exact selected directory. Enabling the new host's Windows autostart removes known legacy host and standalone Window/Taskbar Run entries to prevent duplicate workers.

`core.toml` owns the enabled plugin ids. Restarting a disabled plugin enables it, persists that state and shows `Disable <id>` consistently.

## Follow-up

Post-release designs, default-policy cleanup and conditional technical debt are tracked in [MERGE.md §14](MERGE.md#14-post-v010-todo).
