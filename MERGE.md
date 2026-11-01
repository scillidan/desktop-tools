# Merging the desktop suite — analysis and plan

Status: draft for later review, 2026-11-01.

This document is the detailed reasoning behind the planned merge of:

| Repo | Product | Current state |
|---|---|---|
| `../keyboard-tools-rs/` | keyboard locker, layout switch, layout auto switch-back, two GoldenDict tools | planning; baseline in `../keytools-rs/` |
| `../desktop-tools-rs/` | window/desktop hotkeys, `toggleScreen` | usable |
| `../taskbar-tools-rs/` | Explorer taskbar tools | usable |
| `../desktop-tools-rs-ex/` | OCR/capture/recording | near P1.3, core stable; optional plugin later |

The merge is **not active** yet. This repo is the future home; the gate at the
end of this document decides when to start moving code. Until then, the four
products keep their own repos, builds and releases.

Decisions already taken by the user:

1. One host, one tray icon. Per-plugin tray icons are rejected.
2. Plugins are out-of-process executables with a manifest, not DLLs.
3. Runtime package is a directory; zip is only the distribution wrapper.
4. Product merge first. Repository merge is deferred until the products are
   stable; this repo is the eventual home.
5. The keyboard suite keeps its own name and plan; the merge analysis lives
   here.

---

## 1. Target product

```
desktop-tools.exe                 host: the only tray icon, autostart, plugin lifecycle
  ├─ keyboard-tools.exe           plugin: keyboard locker + layout + GoldenDict tools
  ├─ desktop-tools-rs.exe         plugin: window/desktop hotkeys
  ├─ taskbar-tools-rs.exe         plugin: Explorer taskbar tools
  └─ dt-ocr.exe                   optional plugin: OCR/capture/recording
```

One install, one profile, one autostart entry, one tray icon, one updater.
Each plugin remains a separate process and can still run standalone.

The host is deliberately thin. It owns:

- the tray icon and the whole menu tree;
- the single HKCU Run autostart entry;
- plugin discovery, start/stop/restart and crash backoff;
- the user profile root and log directory;
- the generic actions: enable/disable, restart, edit config, open log,
  configure, reload, about, exit.

It does **not** own:

- hotkeys, hooks or tool business logic;
- plugin config schemas;
- window/taskbar/OCR code.

This keeps the host replaceable and the plugins independently testable.

---

## 2. Plugin runtime: process executables

### 2.1 Options considered

| Option | One process | Hot update | Crash isolation | Language | Verdict |
|---|---|---|---|---|---|
| In-process DLL (`cdylib` + C ABI) | yes | hard | no | Rust only in practice | reject |
| Static Cargo modules (features) | yes | no (rebuild) | no | Rust only | build-time only |
| Process exe + manifest | no (N processes) | yes (stop/replace/start) | yes | any | **chosen** |

### 2.2 Why not in-process DLLs

- **Windows file locks.** A loaded DLL/exe cannot be replaced. Hot update needs
  versioned copies, a stop/unload/rename dance, and a fallback if unload fails.
  A process boundary turns the problem into stop → replace → start.
- **No stable Rust ABI.** `cdylib` plugins must be built with the exact same
  compiler/standard library and share the host's allocator and panic strategy.
  A C ABI with `abi_stable` is possible but adds a second API surface and
  versioning rules; a process contract is simpler and language-agnostic.
- **Blast radius.** Win32 hooks, COM, UIA and OCR worker state are global to a
  process. One bad plugin panics or hangs and the whole suite is gone. The user
  uses these tools all day; losing the keyboard locker because OCR crashed is
  unacceptable.
- **Heavy optional dependencies.** `desktop-tools-rs-ex` pulls WinRT OCR, a
  Python RapidOCR worker and later ffmpeg. It must be installable/absent without
  affecting the host or the other plugins.
- **Tray/menu ownership.** A single tray icon is a host concern. Plugins can
  contribute a menu model over a tiny IPC protocol; they do not need to share
  an address space.

### 2.3 Why not static modules

Static modules are valid for first-party code that is always built together
(e.g. shared primitives). They are not a plugin system: the user cannot install,
remove or update one without rebuilding the suite, and the heavy optional OCR
plugin cannot be kept out of the binary.

### 2.4 Cost accepted

- N processes instead of one. Five key tools plus three desktop tools is a
  handful of small processes, comparable to running the tools separately today.
- A small IPC protocol and a supervisor. That is the price of one tray and
  crash isolation.
- Process startup is milliseconds; plugin startup is not on the hot path.

### 2.5 Revisit triggers

Revisit the process model only if:

- measured IPC latency becomes visible in a hot path (unlikely for hotkeys);
- a plugin needs to share large buffers or GPU surfaces with the host;
- the process count becomes a real memory problem;
- a sandbox is required for untrusted third-party plugins (a different design,
  likely WASM, not an in-process DLL).

---

## 3. Manifest contract

`plugins/<id>/plugin.toml`:

```toml
id = "keyboard-tools"
name = "Keyboard Tools"
version = "0.1.0"
api = 1
exe = "keyboard-tools.exe"
optional = false
tray = "host"          # host | own; merged suite uses host
```

Rules:

- `id` is stable, lowercase, unique; it is the config filename and the menu
  namespace. Never rename it after release.
- `api = 1` is the host/plugin contract version. The host refuses an unknown
  major version and says why.
- `exe` is relative to the plugin directory; absolute paths and `..` are
  rejected.
- `optional = true` means the plugin is not enabled by default and its absence
  is not an error.
- `tray = "host"` means the plugin must not create a tray icon in hosted mode.
  `own` is kept for standalone/debug use.
- The manifest does not carry a second `autostart` flag. `core.toml` is the
  only enable switch; the host owns the single HKCU Run entry.

A plugin directory may also contain a README, default configs, model files and
runtime scripts. The host does not interpret them.

---

## 4. IPC and menu protocol

### 4.1 Transport

Use stdin/stdout with newline-delimited JSON, one object per line.

Why stdio:

- the host already creates the child process and can pipe the handles;
- no port, firewall prompt, discovery or stale socket;
- the pipe lifetime is the process lifetime; closing it is a clear shutdown;
- it works for any language that can read/write standard streams.

Risks and rules:

- **stdout is protocol-only.** All diagnostics go to stderr or the host log.
  A stray `println!` corrupts the stream; the host logs and drops malformed
  lines, and a plugin that violates this must be fixed.
- Max line size 1 MiB; longer lines are rejected.
- No binary data over the protocol; paths are UTF-8 JSON strings.
- The host reads stdout and stderr on separate threads and writes logs.

A named pipe or local socket can replace stdio later without changing the
message schema; the schema is the contract.

### 4.2 Plugin → host events

```json
{"v":1,"event":"ready","menu":[...],"status":"unlocked"}
{"v":1,"event":"menu","items":[...]}
{"v":1,"event":"status","text":"locked"}
{"v":1,"event":"error","message":"hook install failed"}
{"v":1,"event":"log","level":"info","message":"config reloaded"}
```

- `ready` is sent once after the plugin has installed hooks/hotkeys and can
  accept commands. It carries the initial menu and a one-line status.
- `menu` replaces the plugin's menu subtree. The host keeps the last model.
- `status` updates the plugin line in the tray menu/tooltip.
- `error` marks the plugin as degraded but does not kill it unless it exits.
- `log` is optional; stderr is enough for most plugins.

### 4.3 Host → plugin commands

```json
{"v":1,"cmd":"invoke","id":"toggle_lock"}
{"v":1,"cmd":"reload"}
{"v":1,"cmd":"quit"}
```

- `invoke` corresponds to a menu item id the plugin published. The plugin
  handles it on its UI thread, exactly like a tray menu click today.
- `reload` asks the plugin to re-read its config and rebuild hotkeys/hooks.
  If it cannot reload safely, it exits and the host restarts it.
- `quit` is best-effort; the host kills the process tree after a timeout.

### 4.4 Menu model

```json
{"id":"toggle_lock","label":"Lock keyboard","kind":"item","enabled":true}
{"id":"mode_inall","label":"Switch Mode: InAll","kind":"radio","checked":true}
{"id":"swap_esc","label":"Swap ESC/CapsLock","kind":"toggle","checked":false}
{"kind":"separator"}
{"label":"Bound windows: 2","kind":"info"}
```

Kinds:

| Kind | Rendered as | Clickable |
|---|---|---|
| `item` | normal row | yes |
| `toggle` | checkable row | yes |
| `radio` | radio/check row | yes |
| `info` | disabled row (status text) | no |
| `separator` | divider | no |

The host namespaces ids internally (`plugin_id + separator + item_id`), so two
plugins may use the same item id. The plugin id sent back is the original.

Menu limits: 32 items per plugin, label ≤ 120 chars, id ≤ 64 chars. A plugin
that exceeds this is truncated and logged; the host never fails to render.

### 4.5 Why a menu model, not a screenshot/remote menu

- The host renders native Windows menus, so keyboard navigation, high-DPI,
  check marks and accessibility work without custom UI.
- Plugins stay UI-framework-free; the same model can render a native tray menu
  in standalone mode, or a GTK menu in a future Linux host.
- The plugin owns its state; the host owns presentation. No plugin code runs
  in the host process.

---

## 5. Unified tray and menu design

The user is tired of multiple tray icons. The host owns the only icon. The menu
follows Keypirinha's logic: a small, data-driven menu with global actions and a
package picker, not one icon per package.

### 5.1 Right-click menu

```
Desktop Tools
────────────────────────────────────
Plugins ▸
    keyboard-tools        [running] ▸
        Lock keyboard
        Switch Mode: InAll ✓
        Swap ESC/CapsLock
        Clipboard Mode
        ─────────────
        Disable
        Restart
        Edit config
        Open log
    desktop-tools          [running] ▸
        ...
    taskbar-tools          [running] ▸
        ...
    dt-ocr                 [disabled] ▸
        ...
────────────────────────────────────
Configure... ▸
    keyboard-tools.toml
    desktop-tools.toml
    taskbar-tools.toml
    dt-ocr.toml
────────────────────────────────────
Reload
Start with Windows ✓
────────────────────────────────────
About
Exit
```

Rules:

- One icon, one tooltip. The tooltip can summarize: `Desktop Tools — 3 plugins
  running`, with the active alerts (failed/restarting).
- `Plugins` is a submenu; each plugin gets a submenu. This keeps the top level
  short even with eight plugins.
- Plugin-contributed items come first, then a separator, then the generic
  actions (Disable/Restart/Edit config/Open log). The host adds generic actions;
  plugins cannot override them.
- `Configure...` is a submenu of config files (Keypirinha's package picker
  logic without a custom dialog). Clicking one opens it in `$EDITOR` or
  `notepad`.
- `Reload` re-reads `core.toml`, rescans plugins and restarts running plugins
  whose manifest/config changed.
- `Start with Windows` writes the single HKCU Run value for the host.
- `Exit` asks plugins to quit, then kills the process tree after a timeout.

### 5.2 Left-click

Left-click opens the same menu (or the last-used plugin submenu) rather than a
separate window. No custom main window in v1; native menus only.

### 5.3 Failure states

- `failed`: the plugin submenu is still shown, with `Start` instead of
  `Restart`; the error is in the log.
- `starting`: a disabled row `Starting...`.
- `disabled`: generic actions show `Enable` and `Edit config`; no plugin items.
- A plugin that crashes repeatedly is not restarted in a loop: 1 s / 2 s / 5 s,
  max 3 attempts, then `failed` until the user restarts it.

### 5.4 What plugins lose

A plugin can no longer show a custom icon state (locked/unlocked color). The
host status line/menu text carries the state instead. If a plugin truly needs
an icon badge, the protocol can add `status_icon` later; v1 does not.

### 5.5 Notifications

- The host owns the notification identity. A hosted plugin sends
  `{"op":"notify","title":"...","body":"..."}`; the host shows it on its own
  tray icon. `command` and `silent` remain plugin-local escaping hatches.
- The plugin must not call `Shell_NotifyIconW` itself in hosted mode; a
  notification must not create a second tray icon.
- `tray-icon 0.20` gotcha: `TrayIconBuilder::new()` consumes one counter value
  and the platform icon consumes a second, so the first icon's internal `uID`
  is 2. Use `TrayIconBuilder::default().with_id("desktop-tools")` to keep the
  internal `uID` at 1, and make `Tray::notify` return/log failure instead of
  silently targeting a non-existent icon.
- Default `system` uses the host balloon/toast; if Windows Focus Assist hides
  it, `command` (snoretoast) or `silent` is the fallback. Linux later can use
  `notify-send`/dunst through the same command hook.

---

## 6. Profile and configuration

Reference structure:
`C:\Users\User\Share\dotfiles.win\Scoop\keypirinha\portable\Profile\User`
(one user profile folder, one file per package, plus a small core file).

```
Profile/User/
├── core.toml
├── keyboard-tools.toml
├── desktop-tools.toml
├── resize.toml              # desktop-tools owns a second file
├── taskbar-tools.toml
└── dt-ocr.toml
```

Resolution: `--profile <dir>` > `DESKTOP_TOOLS_PROFILE` > `<exe_dir>/Profile/User`.

Rules:

- Defaults are embedded in each plugin (`include_str!`) and written on first
  run. The profile stores only user files.
- The host owns `core.toml`; plugins own their files. The host does not parse
  plugin settings; it only watches mtime and restarts the plugin (v1).
- Old exe-adjacent TOMLs are imported once, never overwritten.
- The profile is gitignored in repos; the user's dotfiles copy is tracked, like
  Keypirinha.
- TOML, not INI. The structure is Keypirinha's; the format stays consistent
  with the existing Rust products.

`core.toml`:

```toml
[host]
start_with_windows = true
log_level = "info"
tray = "host"

[plugins]
enabled = [
  "keyboard-tools",
  "desktop-tools",
  "taskbar-tools",
]
```

`dt-ocr` is optional and absent from the default list.

---

## 7. Packaging, updates and rollback

- Runtime: `plugins/<id>/` directory.
- Distribution: zip per plugin plus a suite zip; Scoop/7z extracts.
- Scoop persists `Profile/User` across upgrades and removes the program files
  on uninstall, not the profile.
- Update: stop the plugin, replace its directory, start it. Keep the previous
  version under `plugins/.backup/<id>-<version>/` until the new one reports
  `ready`; roll back on failure.
- The host refuses a plugin with a newer `api` major version and points at the
  host update.
- Optional one-file drop: `plugins/inbox/<id>.zip` extracted to a temp dir then
  renamed; reject `..` and absolute paths; keep the zip for reinstall. This is
  a later convenience, not the v1 runtime.
- No silent remote code execution: updates are user-initiated or Scoop-driven.
- Code signing is an open question; unsigned local plugins are trusted code.

---

## 8. Hot-plug and lifecycle

- Poll `plugins/`, `core.toml` and each profile file every 2 s. No
  `ReadDirectoryChangesW` in v1; a 2 s scan is simpler and cheap for < 20
  plugins.
- New valid plugin dir: add to catalog; start if enabled and not optional.
- Removed dir: stop the process, remove from catalog; keep the user config.
- Changed manifest/exe/config mtime: restart the plugin.
- Invalid manifest: log and skip; the host never crashes.
- Stop with `taskkill /T` or a job object, so OCR workers and other children do
  not outlive the plugin.
- Single instance: the host holds the mutex; standalone plugin runs hold their
  own. A plugin launched by the host gets `--plugin` and must not create a
  second autostart entry.
- Logs: `Profile/logs/<id>.log`, rotated at 1 MiB with one previous file.
  Do not log clipboard contents or query text by default.

---

## 9. Per-product merge notes

### 9.1 keyboard-tools-rs

- Builds the host first and proves the contract with five small plugins.
- One tray, menu protocol, hot-plug, packaging and release happen here before
  the larger products join.
- This is the reference implementation; its `PLAN.md` is the source of truth
  for its own phases.
- `keyboard-locker` is the stress case for the host: in hosted mode it has no
  tray and no status icon. The password (default `unlock`) is the recovery
  path while keys and mouse buttons are swallowed; mouse movement passes.
  The host menu only needs a lock/unlock action and a status line, not a
  per-plugin icon.
- `keyboard-locker` notifications are currently system balloon / custom command
  / silent. When hosted, `system` should route through the host's notifier so
  all plugins share one notification identity; `command`/`silent` stay
  plugin-local. The host can expose this as a small `host.notify` call rather
  than letting each plugin call `Shell_NotifyIconW` itself.

### 9.2 desktop-tools-rs

Touchpoints:

- `plugin.toml` + a package directory.
- `--profile`: `desktop-tools.toml` and `resize.toml` move into the profile;
  keep the exe-directory fallback for standalone.
- `--plugin` / `--no-tray`: suppress its tray icon and its own HKCU Run writes;
  keep every hotkey and the window logic.
- Contribute its menu model (actions, toggles) to the host; remove the Windows
  tray creation in hosted mode.
- Keep X11 behavior; a Linux host is a separate question.

Risks:

- Its config is two files; the profile model must allow per-plugin extra files.
- It is a larger surface (window snapping, virtual desktops, scaling); do not
  rewrite it during the merge.
- Hotkey conflicts with the keyboard suite are possible; the host should show
  registration failures per plugin.

### 9.3 taskbar-tools-rs

Touchpoints:

- `plugin.toml`, `--profile`, `--plugin` / `--no-tray`.
- Keep `--inspect` / `--dump` diagnostics working standalone.
- Keep the single-instance mutex and the restore-on-quit path.
- Contribute its menu model; remove the tray in hosted mode.

Risks:

- Explorer/UIA state is sensitive to restarts; a plugin restart should not
  restart Explorer.
- UIA can hang on a busy shell; the host must not block its UI thread waiting
  for a plugin. Menu commands go over the pipe; no synchronous UIA calls from
  the host.
- Explorer versions differ; keep the existing compatibility checks.

### 9.4 desktop-tools-rs-ex (optional)

Touchpoints:

- `plugin.toml` with `optional = true`; disabled by default.
- `--profile`: `dt-ocr.toml` in the profile.
- `--plugin` / `--no-tray`; keep the overlay/frame behavior.
- Package README documents uv, Tesseract and language packs.

Risks:

- The RapidOCR worker is a child process; kill the tree/job object.
- Heavy model files make the suite zip large; keep it a separate optional zip.
- Do not merge OCR/frame/recording code into the host.
- Wait for a release tag the user has physically verified.

### 9.5 Shared kit

Duplicated primitives across the products:

- key/combo parser;
- config/profile path helpers;
- autostart and single-instance;
- tray/menu scaffolding and icon generation;
- Win32 input/window helpers (partially).

Extraction order after the plugin boundary is stable:

1. `keys::Combo` (pure, easy, tests already exist).
2. Profile resolver + first-run import.
3. Autostart + single instance.
4. Icon generation.
5. Tray/menu model (the host needs the richest version).

Move one consumer per commit; keep each product's behavior tests green. If the
repos stay separate, path dependencies work only when the repos are siblings;
a release needs a tagged version or vendoring. This is a reason the monorepo
question is deferred, not ignored.

---

## 10. Repository strategy

### 10.1 Product merge first (chosen)

- `desktop-tools` is the future home but starts as this analysis repo.
- The four products keep their repos and release cadence until the gate.
- The user gets one install/profile/tray/autostart without a risky code move.

### 10.2 Monorepo later (deferred)

A monorepo would make the shared kit a normal path dependency and one CI run.
It also couples three very different build chains:

- keyboard suite: small, Windows hooks + tray;
- desktop-tools-rs: Win32/COM/X11, larger;
- taskbar-tools-rs: UIA/Explorer, version-sensitive;
- desktop-tools-rs-ex: WinRT OCR, Python, ffmpeg, large artifacts.

Triggers to reopen the monorepo question:

- shared-kit changes require synchronized commits in 3+ repos;
- release coordination becomes the main source of bugs;
- CI/build times are acceptable in one workspace;
- the user wants one version number and one changelog.

Until then, keep the repos separate and the contract versioned.

### 10.3 Build isolation before the monorepo

All these repos currently inherit
`CARGO_TARGET_DIR=C:\Users\User\.cache\cargo`. That means two repos building
the same binary name overwrite each other — during this work the old
`keytools-rs` build replaced the new `keyboard-locker.exe`/`keyboard-switch.exe`
with pre-fix code, which looks exactly like a regression.

Rules until one workspace exists:

- Never build the old/superseded repo. `keytools-rs` now excludes its moved
  apps from the workspace so a stray build only produces `kit`.
- Keep the app names unique or give each repo its own target dir; a
  repo-local `.cargo/config.toml` does **not** beat the global
  `CARGO_TARGET_DIR` environment variable.
- Rebuild from the owning repo last and check the exe timestamp before a
  physical test.
- CI/release builds must set an explicit per-product target dir.

---

## 11. Security and robustness

- **Process isolation is reliability, not a sandbox.** A plugin can do anything
  the user can. Only install trusted plugins.
- Manifest validation prevents path escapes and accidental bad packages, not
  malicious code.
- Zip import must reject `..`, absolute paths, symlinks and oversized entries.
- Do not log clipboard text, dictionary queries or OCR text by default.
- The host must never block its UI thread on a plugin; all IPC is asynchronous
  with timeouts.
- A plugin that floods stdout gets its pipe drained/dropped, not the host
  hanging.
- Autostart is one HKCU Run value; uninstall leaves the user profile intact.
- Updates are user/Scoop initiated; no background download of code.
- Config writes are atomic (temp + rename) to avoid a half-written TOML.
- Every Windows GUI binary declares
  `#![cfg_attr(windows, windows_subsystem = "windows")]`; without it,
  double-clicking opens a console window. Verify the PE subsystem is 2
  (GUI), not 3 (console), in the release check.

---

## 12. Stability gate (when to re-ask about the merge)

Do not start moving the other products until all of these are true:

- [ ] `keyboard-tools-rs` host + five plugins are released with one tray icon,
  hot-plug and a clean-machine install check.
- [ ] `desktop-tools-rs` has a stable release with its current feature set and
  no open P0/P1 blockers.
- [ ] `taskbar-tools-rs` has a stable release and a documented Explorer
  compatibility range.
- [ ] `desktop-tools-rs-ex` reaches a tagged release the user has physically
  verified.
- [ ] The shared-kit inventory is done and the duplicated primitives are known.
- [ ] The user explicitly reopens the merge discussion.

Then re-ask:

1. Monorepo now, or keep product merge + separate repos?
2. Suite name and executable name (`desktop-tools` vs another name).
3. Which optional plugins ship in the default installer.
4. Update channel/signing.
5. Linux host tray, if X11 is still in scope.

---

## 13. Open questions

1. **Suite name.** `desktop-tools` is the repo; the product/EXE name may differ.
2. **Plugin API versioning.** `api = 1` is a major version; do we need a minor
   field for additive menu kinds?
3. **Menu protocol limits.** 32 items per plugin and 1 MiB lines are guesses;
   measure after the first host.
4. **Config schema migration.** Per-plugin version field? The current tools
   have no schema version; add one before the first release.
5. **Hotkey conflict detection.** The host sees registration failures from
   plugins; should it also enumerate global hotkeys to warn before conflicts?
6. **Log privacy.** GoldenDict query text and OCR text must not leak into logs;
   confirm per plugin.
7. **Left-click behavior.** Same menu vs last plugin submenu; test with the
   user.
8. **Signing.** Sign the host only, or every plugin?
9. **Linux host.** X11-only host tray (GTK/AppIndicator) or run plugins
   standalone; Wayland out.
10. **Plugin discovery paths.** Only `<exe>/plugins`, or also
    `%APPDATA%/desktop-tools/plugins` for user-installed plugins?
11. **Uninstall semantics.** Keep `Profile/User` by default; offer a purge
    option.
12. **Third-party plugins.** If ever allowed, the trust model changes; do not
    pretend the current design is a sandbox.

---

## 14. Appendix: rejected alternatives

### 14.1 DLL plugins

Rejected for file locking, Rust ABI instability, crash blast radius and the
inability to keep heavy optional code out of the host. Revisit only for a
specific latency/shared-memory need.

### 14.2 One monolithic binary

Rejected because it couples OCR/WinRT/Python, UIA/Explorer and low-level hooks
into one build and one crash domain. It also makes optional plugins impossible.

### 14.3 Per-plugin tray icons

Rejected by the user: too many icons. One host icon with plugin submenus.

### 14.4 Zip-only runtime

Rejected for v1: extraction, cache, rollback, path traversal and antivirus
friction. Folder is the contract; zip is distribution.

### 14.5 One config file for everything

Rejected: plugins would need to parse each other's settings, and a malformed
plugin section would break the suite. One file per plugin + core is the
Keypirinha model and keeps ownership clear.

### 14.6 Static modules as the plugin system

Rejected: no install/remove/update without a rebuild; cannot ship optional
heavy plugins separately. Still valid for shared first-party primitives.

---

## 15. Change log

- 2026-11-01: initial analysis after the keyboard-tools-rs plan review. User
  chose process plugins, folder runtime, one tray icon, product merge first,
  and a later repo-merge review. This repo created as the future merge home.
