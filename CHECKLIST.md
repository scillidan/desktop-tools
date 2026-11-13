# desktop-tools physical checklist

Only intentional P2 identity/grouping changes need a new physical pass. The
keyboard-tools-rs P1 delta and previous single-repo P2–P6/P8 baselines are
already signed off.

## P2 — merged workspace bootstrap

Build under test: `stage\desktop-tools.exe` (2026-11-06 21:02, 1,213,952 bytes;
SHA-256 `736259b479a114cabc44873b003616607767020b0092494001895468540b7f1c`).

Automated before handoff:

- [x] `scripts\verify.ps1`: fmt check, 64 tests, clippy with warnings denied,
      complete release build and flat staging, all under repo-local `target`.
- [x] Manifest tests cover default/invalid groups and group → native → external
      ordering.
- [x] Isolated CIM smoke on the final build: five children, occupied
      `Ctrl+Alt+Shift+G`, duplicate hosted locker exit 1 without a modal, and
      zero children after a hard host kill.
- [x] Agent restarted the final host with the existing keyboard profile for the
      completed hosted checks. After the user enabled all plugins and signed
      those checks off, agent reran verify + smoke and stopped the host.
- [x] Agent restarted only `keyboard-switch` standalone from the new stage as
      PID 8292 for the accidentally interrupted menu check; no hosted duplicate
      is running.
- [x] After the user's autostart on/off check, both `desktop-tools` and legacy
      `keyboard-tools-rs` Run values are absent as expected in the off state.

Physical:

- [x] One host tray contains a **Keyboard** group with five plugin submenus.
      Native keyboard tools precede both GoldenDict integrations; there is no
      Others group and no old `keyboard-tools-rs` host.
- [x] The existing profile still enables the same three plugins. Open several
      plugin configs/status pages and confirm previous settings remain intact.
- [x] About identifies **desktop-tools**. Start with Windows was toggled on/off;
      the agent verified both new and legacy Run values are absent afterwards.
- [x] For each hosted plugin, Disable → Restart without an intervening Enable
      still produces Running + **Disable `<id>`**. Ordinary Enable/Disable and
      running Restart still work with immediate status/menu updates.
- [x] Rebuilt `keyboard-switch` standalone passed CapsLock switching, its
      single-tray menu and Exit; user signed off P2 on 2026-11-06.
- [x] Host Exit was checked with all plugins enabled: plugins cleaned up within
      about two seconds and no desktop/keyboard host, plugin child or owned
      hotkey remained.

P2 signed off on 2026-11-06; this unlocked P3.

## P4a — independent Shell Keys suppression

- [x] 140 tests, clippy, release/staging and seven-plugin isolated smoke passed;
      the real Shell Keys profile preserved all 67 previous suppressions.
- [x] Real wire IPC verified Suspend Shell Keys checked=false → true → false and
      a clean Quit. Agent started the staged host as PID 20404 with Shell Keys
      explicitly Enabled and seven children.
- [x] The earlier Win+Ctrl+D failure was observed while KeyMap was Disabled;
      no hook-code change was made without a failing enabled-state signal.

- [x] Keyboard contains native **Shell Keys** before GoldenDict; its config entry
      opens `keyboard-shell.toml`. The legacy Window TOML remains unchanged.
- [x] With Shell Keys visibly Enabled, try **Win+Ctrl+D** first: it must not create
      or switch to a virtual desktop. Then Win+E, Win+Shift+S and Win+. must not
      open their shell UI, and releasing Win must not open Start.
- [x] Keyboard → Shell Keys → **Suspend Shell Keys** must become checked and allow
      Win+E / Win+Ctrl+D normally. Select it again to resume; it becomes unchecked
      and both combinations are suppressed. Window → Suspend Group "Window"
      must remain a separate state and must not suspend Shell Keys.
- [x] Ordinary Ctrl+C and Win+Alt+H window snap still work. Host Exit closes all
      seven plugins without leftover processes/hooks.

The follow-up review requested the separate Key Map entry before signing P4a;
P4a/P4b physical verification is therefore combined below.

## P4b — Key Map and alphabetical menus

- [x] Reproduced the reported Shift failure through `ToUnicodeEx`: left-Shift-only
      state produced `3 / . / a`; synchronized generic modifier state produces
      `# / > / A`. Rebuilt and restarted the real eight-plugin profile.

- [x] Native Keyboard order is **Key Map, Keyboard Auto Switch Back, Keyboard
       Locker, Keyboard Switch, Shell Keys**; both GoldenDict tools remain after
       them. Window feature submenus
       are also alphabetical.
- [x] `Keyboard > Key Map > Edit keyboard-keymap config` opens only the static
       `[keymap]` table. The old shell-only content is preserved separately as
       `keyboard-keymap.legacy-shell.toml`.
- [x] In Notepad and WezTerm, type `#alpha`, `#Alpha`, `#alpha.`, `>>star`,
       `#->` and `;em`: expect `α`, `Α`, `α.`, `★`, `→` and `—`, with no residual
       trigger character, no extra space and no recursive replacement.
- [x] Type `#alphx`, Backspace, then `a`: expect `α`. Type `#al`, click elsewhere,
       then type `pha`: expect literal `pha` because mouse/focus resets history.
- [x] Disable Key Map and type `#alpha`: it stays literal. Enable it and repeat:
       replacement resumes. Normal typing, Backspace and Ctrl+C remain usable.
- [x] Automated wire verification (already run, not a physical substitute):
       invalid prefix config notifies once and keeps the process/old recognizer;
       a subsequent valid config reloads; Quit exits 0; 8-plugin smoke passes.

P4 signed off on 2026-11-07; this unlocks P5.

## P3 — native window import and module split

Build under test: `stage\desktop-tools.exe`, SHA-256
`7554b8b8a663a0dd695a4da2b95692f89202d122f526b259777ff117891290b1`.

Automated before handoff:

- [x] `scripts\verify.ps1`: fmt, 131 tests, clippy with warnings denied,
      release and repo-local staging. Eight original real-desktop tests remain
      intentionally ignored by their own attributes.
- [x] Window is split into `window-core`, Rectangle, Resize, Mouse Drag,
      On Top/Bottom, Maximize/Minimize, App Window Switch, Quick Switch,
      Virtual Desktops and the `window-tools` composition root. Geometry,
      state-machine, mouse, COM and original dispatch tests all pass.
- [x] Each feature owns its schema/default file and edit entry. Fresh profiles
      use split defaults; imported legacy configs keep their effective values.
      Cross-file duplicate hotkeys and invalid reloads retain the old config.
- [x] Wire v3 nested menu validation/rendering and hosted id routing pass;
      hosted and standalone use the same module menu model.
- [x] Final-build isolated smoke: six children, occupied `Ctrl+Alt+L` and
      `Ctrl+Alt+Shift+G`, duplicate hosted locker exit 1 without a modal, and
      zero children after a hard host kill.
- [x] Hosted Window IPC on the final staged binary: a real Quick Switch config
      edit released Ctrl+Alt+L and occupied Ctrl+Alt+O; an invalid reload kept
      the previous config; restoring the file restored the original key.
      Suspend → Resume → Quit also passed with normal `code=0` shutdown.
- [x] The `desktop-tools-rs`, `desktop-tools-rs-ex`, `taskbar-tools-rs` and
      superseded `keytools-rs` source repositories have no modifications;
      keyboard-tools-rs contains only the already reviewed P1/P2 changes.
- [x] Staging compares host/plugin SHA-256 with the release binaries. The actual
      staged window plugin was wire-probed and returned all eight nested feature
      submenus, then exited normally on Quit.
- [x] Agent restored the final staged host as PID 28640 after standalone sign-off,
      using the existing keyboard profile; six children are running and no build
      step is pending.
      A corrected wire-v3 IPC harness also passed Suspend/Resume/Quit after
      reading the actual staged menu/status event order.

### Physical pass A — hosted Window + Keyboard

User reported the hosted behavior "基本ok" on 2026-11-07; the final Suspend
Window / no-manual-Reload menu was additionally verified through real wire v3.

- [x] **Structure/config entries:** one host tray has Window before Keyboard.
      Window directly opens Rectangle / Resize / Mouse Drag / On Top/Bottom /
      Maximize/Minimize / App Window Switch / Quick Switch / Virtual Desktops.
      Each has its own Edit config; Resize also has Edit resize presets, and
      On Top/Bottom has Clear all pins. Window root has Undo and Suspend Group "Window"
      (no manual Reload), plus host Disable/Restart/config/log. No intermediate Window (running),
      duplicate plugin config entry or second Window tray.
- [x] **Rectangle edges/corners:** on a normal window, each Win+Alt+H/L/K/J
      cycles 1/2 → 2/3 → 1/3. Check all four Ctrl+Win+Alt+H/K/J/L corners and
      Ctrl+Win+Alt+C centering. Repeating an edge must not open a shell action.
- [x] **Display/maximize:** Ctrl+Win+Alt+N/P move the same window between
      monitors without changing its relative size; Ctrl+Win+Alt+F maximizes and
      restores it. With one monitor, N/P wrapping to the same monitor is N/A.
- [x] **Pin/minimize/sizer/undo:** Ctrl+Win+Alt+T pins above other windows and
      toggles off; Ctrl+Win+Alt+B keeps a window behind after another window is
      activated and toggles off; Ctrl+Win+Alt+M minimizes; Ctrl+Win+Alt+S opens
      Sizer and applies a preset. Ctrl+Win+Alt+U successively restores Sizer,
      snap, maximize and minimize states. On Top/Bottom → Clear all pins clears pins.
- [x] **Mouse drag:** hold the configured drag modifier and left-drag a normal
      window across a monitor boundary; repeat on a maximized window. The first
      movement restores it without jumping, a modifier-click without movement
      does not restore it, and releasing Win must not open Start.
- [x] **Switcher:** open three windows of one application. Alt+` and
      Alt+Shift+` cycle them forward/back without two-window ping-pong. Close
      one window during the sequence and continue: no error or stuck state.
- [x] **QuickSwitch:** with an Explorer folder active, open a Notepad Save As
      dialog and confirm it follows that folder. Ctrl+Alt+L applies the current
      folder again. Outside a file dialog, Ctrl+Alt+L still reaches the
      application (for example Double Commander list view).
- [x] **Virtual desktops:** with at least three desktops, Win+` / Win+Shift+`
      move next/previous; check every configured Win+Alt+number for desktops
      that exist, then Win+Shift+number moves the active window there without
      losing it or unexpectedly switching focus.
- [x] **P3 compatibility:** Win+E, Win+C, Win+1, Win+Left and Win+Shift+S remain
      suppressed rather than opening shell UI; Win+F2 still turns displays off
      and input wakes them. These are temporary P3-owned behaviors: P4 performs
      the exhaustive suppression matrix after moving it to keyboard-shell, and
      P6c moves monitor power to screen-power.
- [x] **Lifecycle/config:** Window → Suspend Group "Window" disables its hotkeys/drag while
      keyboard tools still work; Resume restores them. Open all eight module
      config entries and confirm each opens its matching TOML with the existing
      effective bindings/settings (the original full legacy file is unchanged).
      Make one harmless gap edit in Rectangle, save, and confirm auto-reload
      applies the new snap margin within about a second; restore and save again.
      Host Restart preserves Running +
      Disable `window-tools` with no duplicate hook or tray.

### Physical pass B — standalone Window (agent will restart between modes)

Agent stopped the host and started only the final staged standalone
`window-tools.exe` as PID 3044; host count is zero.

- [x] With the host stopped by the agent, standalone `window-tools` shows the
      same eight module submenus and state icons, performs one snap and mouse
      drag, and its own Quit exits cleanly without killing other applications.
      User signed off on 2026-11-07.
- [x] Toggle standalone Start with Windows on/off. Agent verified that all six
      current/legacy Run values are absent and no host/window process remains.

P3 is signed off; P4 may proceed.

## P5 — hosted Taskbar

Automated before handoff:

- [x] `scripts\verify.ps1`: 175 tests, clippy, release and hash-verified staging.
- [x] Taskbar geometry/pin behavior tests remain green after extraction. Real
      wire v3 verifies three described feature submenus, `Ctrl+Alt+F9`
      occupancy, Suspend on/off, nested Unpin and Quit=0.
- [x] Isolated and real profiles migrate aggregate Window/Taskbar TOMLs to
      independent files without overwriting existing modules; active aggregates
      are absent and `.legacy.toml` backups retain their previous contents.
- [x] Real Explorer `--dump`, `--inspect` and `--once` return successfully.
- [x] Isolated host starts nine children; Window, GoldenDict and Taskbar hotkey
      probes pass; duplicate hosted locker and hard-kill cleanup still pass.

## P5 Physical — passed

Hosted pass A and standalone pass B were signed by the user. The final host was
rebuilt and restarted as PID 43336 with nine plugins. Verified items:

- [x] Taskbar (running) has Center Taskbar / Hidden Taskbar / Taskbar Pin, each
      with description → separator → its own Edit config. There is no aggregate
      config entry or second tray.
- [x] Centering, Brave pin order, 100 ms polling, Ctrl+Alt+F9 and snapped-window
      resizing preserve the migrated values.
- [x] Suspend Group "Taskbar" pauses Center and Pin as well as the Hidden hotkey;
      Center restores its saved layout while suspended and Resume reapplies the
      configured behavior without overriding a manual Unpin all.
- [x] Agent verified hidden-state graceful Disable/Enable and hotkey recovery.
- [x] Standalone Start with Windows was toggled on/off; no Run value remains.
- [x] Standalone stable MenuId events reach Suspend, autostart, Edit and Exit;
      worker shutdown completes and no taskbar-tools.exe remains.
- [x] Host was restored after standalone testing.

## P6a Physical — passed

- [x] Tray tooltip and Quit item identify Screen Tools; only one Screen tray is present.
- [x] Ctrl+Alt+D draws a frame, Ctrl+Alt+S resizes it, and Ctrl+Enter captures/copies and OCRs ordinary on-screen text.
- [x] Ctrl+Alt+R starts/stops a short recording; output and existing media are intact.
- [x] Screen was stopped and the desktop host restored with nine plugins.

Automated: 116 old-workspace tests, clippy, release, Ctrl+Alt+S occupation, new-instance duplicate rejection, legacy config byte preservation, and real `screen-tools-rs.*` window classes pass. The documented `tools/physical.ps1` does not exist and was not claimed as run.

## P6b Screen hosted baseline

Automated: 290 passed / 11 ignored workspace tests, clippy, release/staging and
ten-plugin smoke pass. All original Screen tests moved with Record (6), Capture
(5), OCR (23) and Draw (37). Real wire-v3 verifies four described feature
submenus, hotkey ownership, Suspend/Resume and clean Quit; a real staged
frame/capture/OCR/result cycle also passes. Split-config probes verify legacy
migration, Draw hotkey replacement, same-process reload and atomic rollback on
invalid OCR TOML. The real profile preserves every effective legacy value and a
byte-identical `.legacy.toml` backup. Multi-monitor and trailing-Draw red/green
probes remain green. Hosted delivery used PID 21340 with ten children.

- [x] Hosted physical pass accepted four alphabetical feature submenus and their
      separate config entries. Logs confirm draw, resize, capture/clipboard/OCR,
      recording, Pin, group Suspend and a clean restart; no modal error appeared.
- [x] On the secondary monitor, both pointer drawing and Ctrl+Alt+W place the Frame there; the drawn Frame remains visible after mouse-up instead of reopening Draw, and moving/resizing across the monitor boundary is not clamped back to the primary display.

## P6b Physical pass B (standalone Screen, delivered PID 20976) — passed

The host and other plugins are intentionally stopped. The staged standalone
Screen process owns `Ctrl+Alt+D` and reads the same split profile files.

- [x] The single Screen Tools tray shows Capture / Draw / OCR / Record with
      descriptions, separators and Edit config entries, plus Screen actions,
      `Suspend Group "Screen"` and `Exit screen-tools`.
- [x] Click `Edit Draw config`; it must open `screen-draw.toml`, proving the
      standalone native-menu event path rather than only the hosted IPC path.
- [x] Toggle Suspend on/off: its checked state changes, `Ctrl+Alt+D` is released
      while checked and works again after resume; draw one frame after resume.
- [x] Click `Exit screen-tools`; the agent verified PID 20976 and every other
      standalone Screen process exited, then restored hosted PID 25072 with ten
      plugins. P6b is signed off and P6c may start.

## P6c Physical — Power transfer (signed off; host PID 23220)

- [x] Screen contains **Power** between OCR and Record, with a grey description,
      separator and `Edit Power config`; it opens `screen-power.toml` containing
      the existing `win+f2` binding.
- [x] Press `Win+F2`: every monitor turns off; ordinary input does not immediately
      turn it back on during forced-off mode. Press `Win+F2` again to wake and
      clear that mode. No Frame is created by this hotkey.
- [x] Suspend Group "Screen" releases `Win+F2`; Resume reclaims it. Window's Undo
      binding and other Screen features remain independently usable.
- [x] Agent verified the final binaries: one-time custom-binding import leaves
      `window-actions.toml` byte-identical, Screen owns the migrated hotkey,
      Window releases it, both plugins Quit with code 0, and ten-plugin smoke passes.

## P6c Physical — Scale transfer (current host PID 12684)

- [ ] Window → Resize contains separate **Edit Resize config**, **Edit Scale
      config** and **Edit resize presets** entries. Scale opens `window-scale.toml`
      with the existing `ctrl+alt+z` binding; Resize still opens its unchanged file.
- [ ] Focus a normal window and press `Ctrl+Alt+Z`: a fullscreen live preview of
      that window appears. Press the hotkey again (or left-click) to dismiss it;
      ordinary source-window contents remain visible in the preview.
- [ ] With a Screen Frame active, `Ctrl+Alt+Z` zooms that Frame. Repeat without a
      Frame and confirm the foreground application window is zoomed instead.
- [ ] Suspend Group "Window" releases Scale; Resume restores it. Screen hotkeys
      and Window Resize/Sizer remain independently usable.
- [x] Agent verified the final binaries: Screen releases the old action/hotkey;
      Window imports a custom binding without rewriting Resize or Screen actions;
      the real DWM overlay toggles on/off, Quit with an active overlay exits with
      code 0 and leaves no overlay, and both plugins Quit cleanly. Full workspace
      tests, clippy, OCR and ten-plugin smoke pass.
