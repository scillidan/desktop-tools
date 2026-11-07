# desktop-tools — merge architecture and staged plan

Status: **design for review, 2026-11-06**. This revision supersedes the
2026-11-01 product-merge-only draft. No product code has been moved or changed
by this revision.

This document is the gate before implementation. Each implementation phase is
small, separately reviewable and separately verified. A phase is not complete
until its automated checks and its hand-off physical checklist pass.

## 1. Decisions confirmed for this revision

1. Merge the products into this repository as **one Cargo workspace**, with
   four functional groups: `window`, `screen`, `keyboard`, `taskbar`.
   A group is a code/config/menu ownership boundary, not necessarily a process.
2. Keep the previously accepted runtime architecture: **one host, one tray,
   out-of-process executable plugins**. Do not replace it with a DLL plugin
   system or one monolithic process. Related modules may share a process when
   forcing a group boundary into IPC would add state without a user benefit.
3. The authoritative keyboard source is `../keyboard-tools-rs`, including its
   host, kit and five plugins. `../keytools-rs` is a historical baseline and
   must not be built or copied over the newer code.
4. Rename `dt-ocr` to **`screen-tools-rs`** and split its implementation into
   focused draw/capture/OCR/record/power modules. `screen-tools` remains the
   thin suite composition root; module splitting does not imply five processes.
5. Put external integrations in their functional domain, after native tools in
   README, code/registration and tray order:
   - window: `Win+E` → `ToggleDoubleCommander`, ``Ctrl+` `` → `ToggleWezTerm`;
   - screen: selected frame → Flameshot handoff;
   - keyboard: native keyboard tools and KeyMap before the GoldenDict tools.
   There is no separate Others group. ``Alt+` `` and ``Alt+Shift+` `` retain
   their existing window-cycling roles.
6. Add rectangle `swapWindows`: select A, then select B; the applications
   exchange their complete rectangles. The two occupied slots therefore stay
   unchanged. This is the Workspacer-like interpretation, not a center-point
   swap that preserves each application's original width/height.
7. Add a small native Rust keymap engine to the keyboard group, using
   [Gustice/AHK-KeyMap](https://github.com/Gustice/AHK-KeyMap) only as a
   behavioral reference. Users define literal trigger → Unicode replacement
   mappings with any symbol prefix. Expansion is immediate, leaves no trigger
   or ending character, and never uses the clipboard. Calculator, dynamic
   dates, special AltGr/numpad handling and decimal clipboard conversion are
   explicitly out of scope.
8. The current phase produces this design only. Product changes wait for
   review of this document.

## 2. Audited starting point

All six repositories (`desktop-tools`, `desktop-tools-rs`,
`desktop-tools-rs-ex`, `keytools-rs`, `keyboard-tools-rs`,
`taskbar-tools-rs`) had clean working trees when audited on 2026-11-06.

| Source | Keep as authoritative | Relevant state |
|---|---|---|
| `desktop-tools-rs` | yes | Window/desktop actions, QuickSwitch, monitor power, Win-combo suppression, Windows + X11 backends |
| `desktop-tools-rs-ex` | yes | Frame, capture, OCR, pinned screenshots, recording, external handoffs, DWM window zoom; already released as v0.1.0 |
| `keyboard-tools-rs` | yes | Host, profile, lifecycle, menu IPC, five keyboard/GoldenDict plugins; previous single-repo P8 physical baseline and merge-P1 delta are signed off |
| `keytools-rs` | historical only | Its five apps predate P1–P8 fixes and are excluded from its workspace |
| `taskbar-tools-rs` | yes | TaskbarPin, CenterTaskbar, HiddenTaskbar, diagnostics and restore-on-quit behavior |
| `desktop-tools` | destination | Planning only before this merge |

Important consequences:

- Do not import the old `keytools-rs/apps/*`: doing so would lose profile,
  hosted-mode, low-level hook, password-lock and menu-model fixes.
- Do not build the old `keytools-rs` apps. The repositories currently inherit a
  shared `CARGO_TARGET_DIR`, and equal binary names can overwrite newer exes.
- The host must be physically verified before more plugins depend on it.
- Existing behavior-sensitive Win32/UIA/hook/OCR code is imported, not rewritten
  for style. Refactors follow behavior-preserving imports and physical checks.

## 3. Target architecture

### 3.1 Source layout

```text
desktop-tools/
├── Cargo.toml
├── Cargo.lock
├── crates/
│   ├── kit/                    # shared keys/profile/menu/IPC/tray/Win32 helpers
│   └── plugin/                 # manifest, catalog, lifecycle, core config
├── apps/
│   └── desktop-tools/          # desktop-tools.exe: host and only suite tray
├── domains/
│   ├── window/
│   │   ├── window-tools/       # window-tools-rs.exe; window actions + composition
│   │   └── app-toggle/         # Double Commander / WezTerm; linked locally
│   ├── screen/
│   │   ├── screen-draw/        # selection, frame, overlay, per-desktop state
│   │   ├── screen-capture/     # frame/window capture, save/copy, screenshot pin
│   │   ├── screen-ocr/         # engines, auto OCR, result/mask windows
│   │   ├── screen-record/      # video, audio, FFmpeg lifecycle
│   │   ├── screen-power/       # monitor power-off and wake recovery
│   │   ├── flameshot-handoff/  # external handoff; linked locally
│   │   └── screen-tools/       # thin screen-tools-rs.exe composition root
│   ├── keyboard/
│   │   ├── keyboard-locker/
│   │   ├── keyboard-switch/
│   │   ├── keyboard-auto-switch-back/
│   │   ├── keyboard-keymap/    # KeyMap + shell-shortcut suppression
│   │   ├── goldendict-search-in-group/
│   │   └── goldendict-auto-search/
│   └── taskbar/
│       └── taskbar-tools/      # taskbar-tools-rs.exe
├── scripts/
├── Profile/User/               # runtime files, gitignored
├── CHECKLIST.md
└── MERGE.md
```

Source domains are ownership boundaries. The runtime package remains flat:

```text
desktop-tools.exe
plugins/<plugin-id>/plugin.toml
plugins/<plugin-id>/<plugin>.exe
Profile/User/*.toml
Profile/logs/*.log
```

Staging flattens executable crates in `domains/*/*` into `plugins/<id>/`.
Library crates such as `screen-draw` and `app-toggle` are linked into their
owning executable and produce no runtime plugin directory. This preserves the
existing catalog discovery/update contract instead of inventing nested runtime
plugin discovery.

### 3.2 Runtime processes

The full suite is the host plus nine plugins (ten processes):

| Primary group | Plugin id | Executable | Default profile |
|---|---|---|---|
| window | `window-tools` | `window-tools-rs.exe` | `window-tools.toml`, `window-resize.toml` |
| screen | `screen-tools` | `screen-tools-rs.exe` | `screen-tools.toml` |
| keyboard | `keyboard-locker` | `keyboard-locker.exe` | `keyboard-locker.toml` |
| keyboard | `keyboard-switch` | `keyboard-switch.exe` | `keyboard-switch.toml` |
| keyboard | `keyboard-auto-switch-back` | `keyboard-auto-switch-back.exe` | `keyboard-auto-switch-back.toml` |
| keyboard | `keyboard-keymap` | `keyboard-keymap.exe` | `keyboard-keymap.toml` |
| keyboard | `goldendict-search-in-group` | `goldendict-search-in-group.exe` | `goldendict-search-in-group.toml` |
| keyboard | `goldendict-auto-search` | `goldendict-auto-search.exe` | `goldendict-auto-search.toml` |
| taskbar | `taskbar-tools` | `taskbar-tools-rs.exe` | `taskbar-tools.toml` |

`window-tools` loads `app-toggle` and `screen-tools` loads
`flameshot-handoff`; neither adds a process. App toggles use window's local
focus/hook helpers and `[double_commander]` / `[wezterm]` in
`window-tools.toml`. Flameshot uses the live screen frame and `[flameshot]` in
`screen-tools.toml`, without IPC. Lifecycle controls stay with the owning
executable, and external actions appear after native actions in its menu.

The keyboard tools remain separate processes. Combining their hotkey ids,
message loops and low-level hooks would be a rewrite, would remove per-tool
lifecycle controls, and would make a GoldenDict/clipboard failure capable of
taking down the keyboard locker. The two GoldenDict tools may be reconsidered
as a pair later, but that is not part of this merge.

Every plugin remains usable standalone. Hosted mode uses
`--profile <dir> --plugin --no-tray`; standalone mode retains its local tray
and diagnostics. Hosted stdout is protocol-only; diagnostics go to stderr.

### 3.3 One tray, four functional groups

Add a validated primary `group = "window" | "screen" | "keyboard" |
"taskbar"` to each executable manifest and retain `external = true` for external
integrations. The host orders native tools/actions first, then external ones;
README and code/registration tables use the same order. Item ids remain
namespaced by the contributing plugin.

The host menu becomes:

```text
Window ▸
    <native window actions>
    ─────
    Toggle Double Commander
    Toggle WezTerm
    ─────
    Disable / Restart / Edit configs / Open log
Screen ▸
    <native screen actions>
    ─────
    Send selected frame to Flameshot
    ─────
    Disable / Restart / Edit config / Open log
Keyboard ▸
    Keyboard Locker ▸
    Keyboard Switch ▸
    Keyboard Auto Switch Back ▸
    KeyMap ▸
    GoldenDict Search In Group ▸
    GoldenDict Auto Search ▸
Taskbar ▸
    <native taskbar actions>
    ─────
    Disable / Restart / Edit config / Open log
─────
Reload core
Start with Windows
About
Exit
```

Single-plugin primary groups render native items first and external integrations
last. Keyboard uses one nested submenu per tool. The host continues to namespace
item ids by plugin; two plugins may use the same local item id.

`screen-tools` is installed and enabled in the full suite so `toggleScreen`
keeps working by default. Its external OCR engines remain optional runtime
dependencies. A suite variant without the screen package may omit it from
`core.toml`; manifest `optional` controls whether absence is an error, while
`[plugins].enabled` remains the only enable/disable switch.

## 4. Domain ownership and migration rules

### 4.1 Window

`window-tools` receives the current `desktop-tools-rs` window functionality:

- rectangle snapping, sizing, minimize/maximize, always-on-top/bottom;
- virtual desktops and same-application window cycling;
- mouse drag, QuickSwitch file dialogs and undo;
- two-window `swapWindows`;
- `scale.rs` from the old `dt-ocr`, moved as a separate reviewed change.

**Confirmed P3b structure (2026-11-06).** Keep one `window-tools`
process, but split the old monolith into native modules with explicit ownership:

| Module | Owns | Later additions |
|---|---|---|
| `window-rectangle` | snapping/centering, display moves and complete rectangles | `swapWindows` |
| `window-resize` | Sizer and explicit resize presets | `scale` |
| `window-mouse-drag` | modifier drag and maximized-window restore | — |
| `window-on-top-bottom` | On Top, On Bottom and one shared Clear all pins action | — |
| `window-maximize-minimize` | Maximize/restore and Minimize | — |
| `window-app-switch` | same-application next/previous window cycling and its foreground-window state | UI: App Window Switch |
| `window-quick-switch` | file-dialog tracking, remembered file-manager folders and auto-fill | UI: Quick Switch |
| `window-virtual-desktops` | virtual-desktop switching and moving windows between desktops | UI: Virtual Desktops |
| `window-tools` | global Undo history, config assembly, hook/action dispatch, shared Win32 loop, menu/profile/IPC and standalone entry | composition only |
| `app-toggle` (external) | no native window actions | Double Commander and WezTerm in P7 |

These modules share the current keyboard/mouse hooks, Win32 message loop,
foreground-window state and reload/suspend lifecycle. Separate exes would
duplicate those mechanisms without a required independent lifecycle or fault
boundary. P3c gives each module its own schema/parser/tests/default/edit entry.
The composition root validates cross-file hotkey conflicts and reloads all
modules atomically. Fresh profiles use split defaults. An existing imported full
`window-tools.toml` stays authoritative only for modules without a split file;
first use of an edit entry materializes that module's effective config without
rewriting the legacy file or any other module.
P3c names align with the module crates: `window-rectangle.toml`,
`window-resize.toml`, `window-mouse-drag.toml`, `window-on-top-bottom.toml`,
`window-maximize-minimize.toml`, `window-app-switch.toml`,
`window-quick-switch.toml` and `window-virtual-desktops.toml`. Global Undo and
legacy compatibility stay in `window-tools.toml`; resize presets keep their
existing separate file. Do not invent per-module enable switches in P3b/P3c.

Recommended menu shape:

```text
Window
├── Rectangle
├── Resize
├── Mouse Drag
├── On Top/Bottom
│   ├── Clear all pins
│   └── Edit On Top/Bottom config
├── Maximize/Minimize
├── App Window Switch
├── Quick Switch
├── Virtual Desktops
├── (P7) Double Commander / WezTerm — external, after native modules
├── Undo Window Action
├── Suspend Window
├── Disable / Restart window-tools
├── Edit window-tools config (one host-owned entry)
└── Open window-tools log
```

Clear all pins is a feature-scoped action, not Restart/Reload. Stop/Restart
share its cleanup primitive so no pinned HWND is orphaned; Reload preserves
pins. Global Undo can reverse both geometry and window-state actions. Config
files auto-reload about one second after saving; no manual Reload menu is needed.
Restart remains the root-level process/hook recovery action.

A small internal `window-core` crate owns only domain primitives (`WinId`,
`Rect`, `Monitor`), narrow window-query traits and the shared binding parser.
It has no tray, IPC, feature policy or host knowledge; feature crates must not
depend back on `window-tools`.

The nested module menu requires an explicit menu-model/host rendering change;
it must not be emulated with fake plugin ids or extra processes. Standalone
uses the same module model. The existing P3a single-plugin menu is only the
behavior-import baseline, not the desired final structure.

The executable also links `window/app-toggle`. Its code and configuration are
window-owned, its actions follow native window actions in the menu, and its
hotkeys stay local to the process rather than being routed through the host.

`toggleScreen` leaves this plugin for `screen-tools`. The default
`desktop-tools.toml` migrates to `window-tools.toml`; `resize.toml` migrates to
`window-resize.toml`. Legacy names are imported once, never overwritten.

The existing ``Alt+` `` / ``Alt+Shift+` `` bindings remain unchanged. `Win+E`
is installed by window's app-toggle module; it must not also remain in the
keyboard suppression list once that toggle is installed.

### 4.2 Screen

`screen-tools-rs` remains the executable and suite name, but its current large
modules are separated into library crates with explicit inputs/outputs:

| Crate | Owns | Must not own |
|---|---|---|
| `screen-draw` | draw overlay, persistent frame, resize/move/pin-click-through, per-desktop frame lifecycle | OCR, capture encoding, FFmpeg, external processes |
| `screen-capture` | frame/window target and capture, image copy/save, screenshot pin and its styles | OCR result policy, recording state |
| `screen-ocr` | backend selection, worker, auto OCR, mask/result editing | frame/window procedure internals |
| `screen-record` | capture loop, audio, FFmpeg, stop/mux and disk limits | OCR/capture action dispatch |
| `screen-power` | `toggleScreen`, monitor/window recovery timer | region selection or capture |
| `flameshot-handoff` | selected-frame → Flameshot bridge, daemon/session state and failure recovery | capture policy or frame lifecycle |
| `screen-tools` | config assembly, hotkeys, action/state coordination, menu/profile/IPC, standalone entry | feature implementation details |

Use ordinary typed Rust interfaces such as `Rect`, `FrameId`, `CaptureImage`,
`OcrRequest` and `RecordSession`; do not introduce a generic event bus. The
composition root explicitly enforces cross-feature rules already present in the
product: recording blocks other actions, capture dismisses stale OCR masks, and
retargeting restarts auto OCR. Module boundaries must make those transitions
visible rather than hide them behind globals.

Configuration remains one `screen-tools.toml`, with consistent sections such as
`[draw]`, `[capture]`, `[ocr]`, `[record]`, `[pin]` and `[power]`. The parser and
defaults are shared; each module owns its section schema and validation. A
one-time legacy import preserves existing user values. This is structural
unification, not five copies of config/tray/menu plumbing.

The DWM window zoom (`scale`) belongs to window and moves out. Flameshot is a
separate screen-owned handoff module and uses `[flameshot]`; the normal
image-editor handoff stays with capture unless separately requested. Both
external handoffs follow native capture actions in code and menu order. The
per-desktop frame lifecycle stays in
`screen-draw`; it may use shared virtual-desktop primitives. `frame_window` is
a capture/draw action even though it uses window-discovery helpers.

The names “Pin frame (click-through)” and “Pin screenshot” must be used in the
merged menu so they cannot be confused with window “Always on top”.

**Why not five processes now:** these modules share one live selection and
cross-feature state. Separate exes would require a selection-session RPC,
cross-process overlay exclusion and coordinated cancellation merely to preserve
current behavior, without a new user-visible benefit. TODO: reconsider separate
processes only when independent installation/lifecycle or fault isolation is an
actual requirement; that change first requires an explicit selection-session
protocol and its physical test matrix.

### 4.3 Keyboard

The five current `keyboard-tools-rs` plugins move unchanged first. The new
`keyboard-keymap` plugin then owns:

- user-defined literal trigger → Unicode replacement mappings;
- shell shortcuts formerly written as `"combo" = "suppress"` in
  `desktop-tools.toml`, isolated as the **Shell Keys** module in the same
  process, with its own editable `keyboard-shell.toml`; no new process or
  top-level Others group is introduced.

The generic low-level Win-combo hook implementation belongs in `crates/kit`,
not in a cross-process hotkey broker. `window-tools` and `keyboard-keymap`
each install it for their own local binding table and execute their own
actions. There is no hook → host IPC → plugin round trip on a hotkey path.

Only the shell-suppression policy moves to keyboard. Window-specific Win
bindings stay local to window; otherwise snapping and desktop actions would
gain IPC latency, stale binding-index races and a hard dependency on another
process.

### 4.4 Taskbar

`taskbar-tools` imports its modules without rewriting their behavior-sensitive
internals:

- physical drag and pin matching/order;
- UIA mutex and COM drop order;
- native/legacy centering and registry restoration;
- hidden-taskbar restore and snapped-window handling;
- `--dump`, `--inspect` and `--once` diagnostics.

Standalone diagnostics keep their working console behavior. In hosted mode,
logs and errors go to stderr, never to stdout or a modal message box.

### 4.5 External integrations and ordering

External applications do not require a separate top-level group. Window owns
Double Commander/WezTerm toggles, screen owns the Flameshot bridge, and keyboard
owns GoldenDict integration. Each domain lists native modules, configuration
sections and menu actions first, then external integrations. Manifest
`external = true` supplies the same ordering for discovered executable plugins.

`window-tools` invokes `app-toggle` directly with its normal Win32 window/hook
primitives. `screen-tools` invokes `flameshot-handoff` directly with the current
frame id and geometry. Neither path serializes a live window or selection
through the host. Logs identify both the external feature and owning process.

Ordinary capture/save/copy, OCR, recording and the non-Flameshot image editor
remain native screen functionality.

## 5. Window application-toggle contract

`window-tools.toml` uses named sections for external applications (the exact
schema is proposed here):

```toml
[double_commander]
key = "win+e"
command = "doublecmd.exe"
process = "doublecmd.exe"

[wezterm]
key = "ctrl+`"
command = "wezterm-gui.exe"
process = "wezterm-gui.exe"
```

Commands may be a PATH-resolved executable or an absolute path. Launch uses an
executable plus argument list, not an interpolated shell command. No arbitrary
argument template is needed in v1.

For each press, query current state rather than relying on a saved visible
flag:

| Current state | Action |
|---|---|
| No matching visible window and no pending launch | Start the configured executable |
| Matching window exists and is foreground | Minimize it |
| Matching window exists but is not foreground | Restore if minimized, show and activate it |
| Launch is pending | Do not launch a duplicate; allow retry after a 5 s timeout |

Matching is case-insensitive on executable basename (`doublecmd.exe`,
`wezterm-gui.exe`). The most recently active/topmost matching visible window is
selected when WezTerm has multiple windows. The toggle does not cycle through
all WezTerm windows.

Focus/restore reuses the existing Win32 focus helper and reports failure to the
plugin status/log. Elevated-window and Windows foreground-lock restrictions are
documented, not hidden.

`Win+E` is handled by the `window-tools` process's local Win hook before
Explorer opens its own file manager. ``Ctrl+` `` uses
normal hotkey registration. Held-key auto-repeat
must be swallowed without repeatedly toggling; repeat policy is per action so
existing repeatable rectangle actions do not change.

## 6. Rectangle `swapWindows` contract

Proposed default binding: ``Ctrl+Win+Alt+X`` (`x` = exchange). This binding is
still subject to design review; the interaction and geometry are confirmed.

### 6.1 State machine

```text
Idle
  swapWindows on eligible A → Armed(A)
Armed(A)
  swapWindows on eligible B ≠ A → Commit(A, B) → Idle
  swapWindows on A               → Cancel → Idle
  Esc                            → Cancel → Idle
  timeout / invalid A / reload / suspend / quit → Cancel → Idle
```

- Armed stores A's stable window id and deadline, not stale geometry.
- The timeout is 15 s. A short status tip says “Swap: select second window;
  Esc cancels”. Hosted mode sends the same state as plugin status text.
- Other rectangle actions cancel Armed before running, avoiding an ambiguous
  implicit second selection.
- Alt+Tab and ordinary foreground changes are allowed; that is how B is chosen.
- A/B must be visible, zonable, neither minimized nor maximized at selection
  and commit. Invalid B cancels with a status message and changes no geometry.
- Esc is swallowed only while Armed. There is no permanent capture state.

### 6.2 Geometry and history

At commit, read both current outer rectangles and DWM visible frames. Expand
B's visible frame by A's own invisible-border insets for A's new outer rectangle;
do the converse for B. Do not apply snap gaps or work-area scaling. This
preserves the two visible slots and works across monitors/DPIs.

The two moves form one composite undo entry. One `undo` restores both original
rectangles; it is not two user-visible undo steps. B keeps foreground focus.

Unit tests cover Idle/Armed/cancel/timeout, same-window selection, invalid or
closed windows, maximized/minimized rejection, cross-monitor rectangles,
per-window DWM insets, repeat hotkeys and atomic undo. Real-window tests are
ignored by default and included in the physical checklist.

## 7. `keyboard-keymap` contract

`Gustice/AHK-KeyMap` is now only a behavioral reference. The requested product
is the small core mapping engine, not a port of the reference's calculator,
dates, AltGr table, numpad workaround or clipboard conversion. No AHK runtime
is required. If example mappings are derived from the reference, retain its
MIT attribution with those examples.

### 7.1 User-defined static mappings

`keyboard-keymap.toml` contains literal, case-sensitive UTF-8 mappings:

```toml
[keymap]
"#alpha" = "α"
"#Alpha" = "Α"
"#->" = "→"
">>star" = "★"
";em" = "—"
```

- `#` is an example, not a reserved prefix. A trigger may start with any
  printable symbol and may contain multiple characters; `>>star` and `;em` are
  first-class mappings just like `#alpha`.
- Replacement is static Unicode text. There is no variable expansion, date
  function, regex, script, process launch or clipboard step.
- Matching is exact and case-sensitive. There is no implicit case conversion,
  word-boundary rule, ending Space/Enter or other hidden syntax.
- An empty trigger is invalid. Empty replacements are allowed and erase the
  complete trigger. Config reload is atomic: invalid input keeps the last good
  map and reports a concise non-modal error.
- If one complete trigger is a prefix of another (for example `#a` and `#ab`),
  immediate matching is ambiguous. Reject that map with an error naming both
  triggers. Do not add a timeout or guess from the next character; the user can
  choose unambiguous triggers. This determinism is part of the stability goal.
- There is no bundled dynamic AHK feature inventory. Users can put the symbols
  they actually need in this file; an example file may contain static mappings.

### 7.2 Exact replacement without a residual trigger symbol

This engine specifically replaces the Espanso workflow that leaves an extra
trigger character. Correct final text and input ordering matter more than a
large built-in symbol catalog.

1. A low-level keyboard hook feeds a bounded, in-memory character buffer. Its
   maximum length is the longest configured trigger; its contents are never
   written to logs or disk.
2. Ordinary characters pass through while the recognizer tracks suffixes. On
   the final character of an unambiguous exact match, swallow that final key,
   send exactly `trigger_character_count - 1` Backspaces for the characters
   already passed through, then send the replacement with `SendInput` +
   `KEYEVENTF_UNICODE`. A one-character trigger needs zero Backspaces.
3. Backspace count is based on input characters, never UTF-8 bytes. Deletion,
   replacement and the next ordinary input are serialized so fast typing cannot
   insert a character between deletion and replacement.
4. The replacement therefore contains no `#`/prefix and needs no ending
   character. Typing `#alpha` produces exactly `α`; typing `#alpha.` produces
   exactly `α.` with one ordinary dot. No Space is injected or consumed.
5. Injected Backspaces and replacement text are marked and ignored by the
   recognizer. A replacement equal to a trigger must still run only once, never
   recurse.
6. Backspace edits the buffer. The buffer resets after replacement, focus
   change, mouse click, arrow/Home/End/Page navigation and modifier chords.
   Before deleting anything, verify that the foreground window is unchanged;
   otherwise leave the typed text alone and abort.
7. Suspend/Resume affects only keymap expansion. It does not unexpectedly
   re-enable shell shortcuts by disabling the separate suppression module.
   Disabling the whole plugin still disables both through the normal lifecycle
   control.

The initial implementation spike must validate `ToUnicodeEx`/scan-code behavior
with the active layout, dead keys and Chinese IME. The LL hook sees keyboard
events, not arbitrary text committed by an IME; IME-composed trigger support is
not claimed without a separate verified design. Higher-integrity applications
remain outside a normal-integrity global hook.

Pure model tests cover every configured mapping, exact output with no residual
prefix/end character, one-character and non-ASCII triggers, empty replacement,
case sensitivity, unmatched suffixes, prefix-conflict rejection, Backspace and
reset rules, focus changes, injected-input recursion, fast following characters,
bounded memory and last-good reload. Physical tests compare exact text in
Notepad, WezTerm, a browser edit control and an Office-style rich edit, including
fast typing and Chinese IME/German layout where available. CopyQ should confirm
that expansion never touches the clipboard.

### 7.3 Shell suppression is separate from KeyMap

The same executable may host the small shell-suppression module so keyboard
hook infrastructure is shared without adding another process. It has its own
`[shell]` config section, validation, menu state and tests; it is not a KeyMap
trigger feature and shares no recognizer state.

- Existing suppress-only Win bindings move here from `desktop-tools.toml`.
- `Win+C` remains suppressed; the reference's calculator mapping is not ported.
- `Win+E` remains suppressed only until the window app toggle replaces it.
- Lock arbitration and repeat rules follow §8. No clipboard, calculator, date,
  AltGr or numpad special case is reintroduced through this module.

## 8. Host and shared-kit prerequisites

These are corrections required by the audit, not optional polish:

1. **Drain IPC on every wakeup.** The current stdout reader posts wakeup 2,
   while the host loop only runs lifecycle work for wakeup 1. Status/menu events
   can otherwise wait for the 500 ms timer.
2. **Graceful stop with timeout.** Current stop writes `quit` and immediately
   force-kills the tree. Introduce a `Stopping` state reaped by the host timer:
   wait up to 2 s, then kill the tree. Taskbar must get time to restore hidden
   and centered state; screen must get time to stop/mux recording. Never block
   the host UI thread for the full timeout.
3. **Hosted failures are non-modal.** Single-instance and startup failures go
   to stderr and exit nonzero; no background plugin waits behind a message box.
4. **Hosted notifications use the host.** Add the specified notify event and
   route system notifications through the single tray identity. Command/silent
   remain plugin-local options. No hosted plugin calls `Shell_NotifyIconW`.
5. **Profile and autostart migration:** `--profile` >
   `DESKTOP_TOOLS_PROFILE` > legacy `KBTOOLS_PROFILE` > exe `Profile/User`.
   Quote autostart executable paths. Migrate old Run values to one
   `desktop-tools` value; do not leave two hosts starting at login.
6. **Editor parity:** shared Edit Config supports `EDITOR` then `VISUAL`, with
   the existing safe quoting behavior. Window's second presets file gets a
   plugin-contributed Edit Presets action.
7. **Lock arbitration:** the keyboard locker publishes a named lock-state
   event/shared flag. Other low-level hooks pass events while it is set so the
   locker can swallow them regardless of hook installation order. Otherwise
   window or layout hooks could still act while the keyboard is locked.
8. **Hotkey repeat policy:** a hook result distinguishes pass, swallow+dispatch
   and swallow-only. Non-idempotent toggles use swallow-only on auto-repeat;
   current repeatable actions retain their behavior.

### Shared-kit extraction order

Import exact copies first, then unify one consumer at a time:

1. profile resolver, legacy import and modified-time helpers;
2. key/combo parser union (`Enter`, `Snapshot`, `Pause`, `AltGr`, `CapsLock`,
   numpad and OEM aliases) with compatibility tests;
3. hotkey registration (both HWND-less and HWND-owned, partial-failure lists,
   suspend/resume);
4. menu model, IPC and native tray rendering;
5. single instance, quoted autostart, editor and notification helpers;
6. generic Win-combo hook and lock gate;
7. geometry only where semantics are actually equal.

Do not force the two different `SizeSpec` models together during the move.
Do not deduplicate taskbar geometry merely because types share the name `Rect`.
Do not rewrite physical drag, UIA/COM, OCR, recording or hook logic for style.

## 9. `screen-tools-rs` rename boundary

The rename is a standalone phase before host integration. It includes all of
the following; renaming only the Cargo package is incomplete:

- workspace/package/exe, plugin id and `screen-tools.toml`;
- one-time import of exe-adjacent/profile `dt-ocr.toml`, never overwriting an
  existing new config;
- log, mutex, temp directories, temp-file prefixes and default output names;
- capture/record default directories with documented old-file retention;
- tray/menu labels, build resource and icon filename;
- every owned Win32 class/title **and** `capture.rs`'s `OWN_CLASSES` in the same
  change, so capture cannot start including the tool's own overlays;
- rebuild/freshness scripts, process cleanup for old and new exe names, CI and
  release archives;
- unit/integration scripts, external physical-test harnesses, README/ROADMAP/
  AGENTS and cross-repo links;
- a new Scoop manifest/release mapping when release work is reached. The old
  AHK `screen-ocr` manifest is a different product and is not overwritten.

Temp cleanup recognizes both old and new prefixes for one release. Old capture
and recording files are never moved or deleted automatically.

`scale` does not move in the rename commit. `dt-common` does not get a separate
cosmetic rename if it will immediately be folded into the merged workspace.

## 10. Implementation phases and gates

No phase starts until the previous phase's checklist is signed off. Commits are
per phase or smaller; there is no single bulk “merge everything” commit.

### P0 — Design record (this phase)

- Record architecture, ownership, feature contracts and phase gates here.
- No product code changes.

Done when the user approves the design, including the proposed swap hotkey and
the static KeyMap contract in §7.

### P1 — Prove and repair the existing host

**Status 2026-11-06: signed off.** §8 items 1–4, disabled→Restart state
consistency and native-before-external ordering passed automated and physical
checks. The previous single-repo P2–P6/P8 baseline is also accepted. Build
identity and results are recorded in `keyboard-tools-rs/CHECKLIST.md` under
“Merge P1 — host readiness”.

Work first in `keyboard-tools-rs`; do not move code yet.

- Apply §8 items 1–4 and their focused tests.
- Complete the existing P8 one-tray physical checklist for all five plugins.
- Record actual exe paths/timestamps/hashes and physical results in CHECKLIST.

Gate: one tray, all menu actions, no hosted dialogs, notifications, graceful
quit and no orphaned children pass physically.

### P2 — Bootstrap `desktop-tools`

**Status 2026-11-06: signed off.** Hosted identity/grouping/lifecycle/autostart,
host exit and the standalone keyboard-switch recheck all passed. See
`CHECKLIST.md`.

- Import `keyboard-tools-rs` host/kit/plugin/five plugins exactly.
- Rename the host to `desktop-tools`, add profile/autostart compatibility and
  four-group menu metadata and native-before-external ordering.
- Stage flat runtime plugins; keep standalone plugin behavior.
- Add explicit repo-local target-dir handling to every build/CI script.

Gate: the same P8 physical checklist passes from `desktop-tools`, with only the
new host identity and grouped menu as intentional differences.

### P3 — Import and modularize window

**Status 2026-11-07:** behavior import, feature ownership and P3c config split
have 131 passing tests (plus eight intentionally ignored real-desktop tests),
clean fmt/clippy/release/staging, six-plugin smoke and real wire-v3 checks for
module reload, invalid-config rollback and Suspend/Resume/Quit. Hosted and
standalone physical passes are signed off. Wire v3 adds nested submenus; use one
staged package for host and all plugins.

- P3a: import `desktop-tools-rs` as `window-tools`; add profile/plugin/no-tray and
  menu IPC.
- Migrate config/presets and autostart once; keep `--action`, `--pid`, `--config`
  and X11 standalone behavior.
- P3b (approved 2026-11-06): extract the §4.1 native modules and their menu structure
  one at a time, with tests after each extraction; do not add feature redesign.
- P3c (approved 2026-11-07, after the P3 behavior baseline): give each native
  module its own config schema/parser/tests and edit entry; validate cross-file
  hotkey conflicts and reload all module configs atomically. Preserve legacy
  `window-tools.toml` compatibility and never silently rewrite user comments.
- Temporarily keep current screen/suppress behavior until their owning phases,
  so there is no feature gap during migration.

Gate: all current window/desktop/QuickSwitch/mouse/undo physical checks pass,
plus hosted Reload/Restart/Exit and standalone parity.

### P4 — Add KeyMap and move shell suppression

Split into reviewable substeps:

- P4a: the separate shell module owns suppression; window ignores migrated
  suppress rows. It temporarily retains `Win+E` suppression until P7 installs
  the app toggle, so Explorer never regains that key during migration. `Win+C`
  stays suppressed. Verify every default Win binding, not a spot check, and
  verify lock arbitration.
- P4b: implement only the user-defined static mapping engine and its exact
  replacement/reset/config-validation state machine. No calculator, dynamic
  dates, AltGr/numpad special cases or clipboard conversion.

Gate: §7's mapping and physical text comparisons pass with no residual/extra
trigger character; no hotkey/hook regression in locker, switch or window; no
trigger/replacement text appears in logs and expansion never touches clipboard.

### P5 — Import taskbar

- Add profile/hosted menu/IPC while preserving diagnostics and standalone mode.
- Keep existing taskbar algorithms unchanged.
- Exercise graceful disable/restart/quit, including forced timeout behavior.

Gate: `--dump`, `--inspect`, `--once`, pin ordering, native/legacy centering,
hide/show, snapped windows and all restore-on-quit paths pass physically with
one host tray and one autostart entry.

### P6 — Rename and import screen

- P6a: perform the complete `screen-tools-rs` rename in the old repo and verify
  the existing OCR/capture/recording physical suite.
- P6b: import into `domains/screen`, add hosted/profile behavior, then extract
  draw/capture/OCR/record/power one module per commit with tests after each.
- P6c: move `toggleScreen` to `screen-power`, `scale` to window, and the
  Flameshot bridge to `domains/screen/flameshot-handoff` as separate commits.
  Keep these calls in their current owning processes; add no frame IPC.

Gate: frame states, OCR engines/results/editing, image copy/save/pin, external
handoffs, recording/audio/stop, monitor power/wake recovery and window zoom all
pass physically after every extraction. Verify that capture excludes every
owned overlay class and that Flameshot still receives the current live frame.

### P7 — Add window application toggles

- Implement `window/app-toggle`: `ToggleDoubleCommander` on `Win+E` and
  `ToggleWezTerm` on ``Ctrl+` `` with the §5 state machine. `window-tools`
  links the module and exposes the actions after native window actions. In the
  same phase, remove KeyMap's temporary `Win+E` suppression so there is neither
  a gap nor a duplicate owner.
- Test absent/minimized/background/foreground/pending-launch states, multiple
  WezTerm windows, auto-repeat, elevated focus failure and lock state.
- Confirm ``Alt+` `` and ``Alt+Shift+` `` still cycle same-app windows.

Gate: every transition is physically checked; Explorer never opens on `Win+E`;
holding either key cannot oscillate the window.

### P8 — Add `swapWindows`

- Implement Armed/cancel/timeout, complete-rectangle exchange and composite
  undo.
- Keep the proposed hotkey disabled until P0 approval explicitly accepts it.

Gate: two apps visibly exchange slots; same-window/Esc/timeout/invalid/reload/
suspend cancel; maximized/minimized cases do nothing; cross-monitor/DPI and
one-step undo pass physically.

### P9 — Consolidate, package and retire old products

- Unify every suite/standalone executable and tray `.ico` in one visual style,
  including normal/suspended/error/disabled states. Keep hosted and standalone
  identities aligned and physically compare the final assets at common DPIs;
  do not mix this cosmetic pass into behavior migrations.
- Apply shared-kit extraction one consumer per commit, repeating affected
  physical checks.
- Produce standalone and suite packages, update Scoop and release workflows.
- Verify clean profile, upgrade with existing profile, uninstall preserving
  profile and rollback of a failed plugin.
- Only after the full suite is signed off, mark old repos archived/read-only.
  Do not delete user profiles, outputs or old repositories automatically.

## 11. Verification and build rules

Automated checks for the merged workspace:

```text
cargo fmt --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release
cargo check --target x86_64-unknown-linux-gnu   # portable standalone crates
```

Every build command/script/CI job sets an explicit repo-local `CARGO_TARGET_DIR`.
A repo-local `.cargo/config.toml` alone does not override the existing global
environment variable. Before each physical hand-off:

1. stop the owning old process/plugin;
2. rebuild from the authoritative repository last;
3. stage a fresh runtime directory;
4. record exe path, timestamp and hash;
5. relaunch that exact staged exe;
6. separate automated evidence from the user's physical checklist.

Hotkeys, hooks, tray state, focus, window geometry, clipboard, OCR, recording,
layouts and Explorer/UIA behavior are never declared physically verified from
unit tests alone. Existing external tools (Hotkey Detective, CopyQ and the
project's PowerShell probes) remain part of the relevant checklists.

## 12. Deferred or rejected work

- **Rejected:** DLL plugins, one monolithic binary, per-plugin tray icons,
  zip-only runtime, one giant config file, centralized host-mediated hotkeys.
- **Deferred:** merging the five keyboard tools into one process; splitting
  screen modules into separate processes until independent lifecycle/fault
  isolation justifies a selection-session protocol; DDA/high-FPS recording;
  Magpie-style shader scaling/input passthrough; merging the two different
  resize-spec languages; hot-plug zip inbox; code signing; Linux host tray;
  Wayland/macOS.
- Existing standalone X11 behavior remains where it already exists. The merged
  host is Windows-first; no Linux regression claim is made without its own
  checks.

## 13. Details awaiting explicit design approval

The architecture and feature semantics above are ready for review. Three proposed
details were not fully specified by the original request and should be called
out in the approval rather than silently accepted:

1. Four functional domains own their external integrations, with native tools
   before external ones in docs/code/menus. Screen is split into focused library
   crates behind one `screen-tools` composition root. Separate processes are
   deferred until independent lifecycle/fault isolation justifies the
   selection-session protocol described in §4.2.
2. ``Ctrl+Win+Alt+X`` as the default `swapWindows` binding.
3. KeyMap matches immediately without an ending character and rejects maps
   where one complete trigger prefixes another. This avoids timing/look-ahead
   ambiguity and is the design intended to eliminate Espanso's residual trigger
   symbol rather than reproduce AHK's unrelated features.

Everything else remains an implementation-phase concern, not a reason to start
a bulk move before this document is approved.
