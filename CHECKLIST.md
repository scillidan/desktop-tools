# desktop-tools physical checklist

Only intentional P2 identity/grouping changes need a new physical pass. The keyboard-tools-rs P1 delta and previous single-repo P2–P6/P8 baselines are already signed off.

## P2 — merged workspace bootstrap

Build under test: `stage\desktop-tools.exe` (2026-11-06 21:02, 1,213,952 bytes; SHA-256 `736259b479a114cabc44873b003616607767020b0092494001895468540b7f1c`).

Automated before handoff:

- [x] `scripts\verify.ps1`: fmt check, 64 tests, clippy with warnings denied, complete release build and flat staging, all under repo-local `target`.
- [x] Manifest tests cover default/invalid groups and group → native → external ordering.
- [x] Isolated CIM smoke on the final build: five children, occupied `Ctrl+Alt+Shift+G`, duplicate hosted locker exit 1 without a modal, and zero children after a hard host kill.
- [x] Agent restarted the final host with the existing keyboard profile for the completed hosted checks. After the user enabled all plugins and signed those checks off, agent reran verify + smoke and stopped the host.
- [x] Agent restarted only `keyboard-switch` standalone from the new stage as PID 8292 for the accidentally interrupted menu check; no hosted duplicate is running.
- [x] After the user's autostart on/off check, both `desktop-tools` and legacy `keyboard-tools-rs` Run values are absent as expected in the off state.

Physical:

- [x] One host tray contains a **Keyboard** group with five plugin submenus. Native keyboard tools precede both GoldenDict integrations; there is no Others group and no old `keyboard-tools-rs` host.
- [x] The existing profile still enables the same three plugins. Open several plugin configs/status pages and confirm previous settings remain intact.
- [x] About identifies **desktop-tools**. Start with Windows was toggled on/off; the agent verified both new and legacy Run values are absent afterwards.
- [x] For each hosted plugin, Disable → Restart without an intervening Enable still produces Running + **Disable `<id>`**. Ordinary Enable/Disable and running Restart still work with immediate status/menu updates.
- [x] Rebuilt `keyboard-switch` standalone passed CapsLock switching, its single-tray menu and Exit; user signed off P2 on 2026-11-06.
- [x] Host Exit was checked with all plugins enabled: plugins cleaned up within about two seconds and no desktop/keyboard host, plugin child or owned hotkey remained.

P2 signed off on 2026-11-06; this unlocked P3.

## P4a — independent Shell Keys suppression

- [x] 140 tests, clippy, release/staging and seven-plugin isolated smoke passed; the real Shell Keys profile preserved all 67 previous suppressions.
- [x] Real wire IPC verified Suspend Shell Keys checked=false → true → false and a clean Quit. Agent started the staged host as PID 20404 with Shell Keys explicitly Enabled and seven children.
- [x] The earlier Win+Ctrl+D failure was observed while KeyMap was Disabled; no hook-code change was made without a failing enabled-state signal.

- [x] Keyboard contains native **Shell Keys** before GoldenDict; its config entry opens `keyboard-shell.toml`. The legacy Window TOML remains unchanged.
- [x] With Shell Keys visibly Enabled, try **Win+Ctrl+D** first: it must not create or switch to a virtual desktop. Then Win+E, Win+Shift+S and Win+. must not open their shell UI, and releasing Win must not open Start.
- [x] Keyboard → Shell Keys → **Suspend Shell Keys** must become checked and allow Win+E / Win+Ctrl+D normally. Select it again to resume; it becomes unchecked and both combinations are suppressed. Window → Suspend Group "Window" must remain a separate state and must not suspend Shell Keys.
- [x] Ordinary Ctrl+C and Win+Alt+H window snap still work. Host Exit closes all seven plugins without leftover processes/hooks.

The follow-up review requested the separate Key Map entry before signing P4a; P4a/P4b physical verification is therefore combined below.

## P4b — Key Map and alphabetical menus

- [x] Reproduced the reported Shift failure through `ToUnicodeEx`: left-Shift-only state produced `3 / . / a`; synchronized generic modifier state produces `# / > / A`. Rebuilt and restarted the real eight-plugin profile.

- [x] Native Keyboard order is **Key Map, Keyboard Auto Switch Back, Keyboard Locker, Keyboard Switch, Shell Keys**; both GoldenDict tools remain after them. Window feature submenus are also alphabetical.
- [x] `Keyboard > Key Map > Edit keyboard-keymap config` opens only the static `[keymap]` table. The old shell-only content is preserved separately as `keyboard-keymap.legacy-shell.toml`.
- [x] In Notepad and WezTerm, type `#alpha`, `#Alpha`, `#alpha.`, `>>star`, `#->` and `;em`: expect `α`, `Α`, `α.`, `★`, `→` and `—`, with no residual trigger character, no extra space and no recursive replacement.
- [x] Type `#alphx`, Backspace, then `a`: expect `α`. Type `#al`, click elsewhere, then type `pha`: expect literal `pha` because mouse/focus resets history.
- [x] Disable Key Map and type `#alpha`: it stays literal. Enable it and repeat: replacement resumes. Normal typing, Backspace and Ctrl+C remain usable.
- [x] Automated wire verification (already run, not a physical substitute): invalid prefix config notifies once and keeps the process/old recognizer; a subsequent valid config reloads; Quit exits 0; 8-plugin smoke passes.

P4 signed off on 2026-11-07; this unlocks P5.

## P3 — native window import and module split

Build under test: `stage\desktop-tools.exe`, SHA-256 `7554b8b8a663a0dd695a4da2b95692f89202d122f526b259777ff117891290b1`.

Automated before handoff:

- [x] `scripts\verify.ps1`: fmt, 131 tests, clippy with warnings denied, release and repo-local staging. Eight original real-desktop tests remain intentionally ignored by their own attributes.
- [x] Window is split into `window-core`, Rectangle, Resize, Mouse Drag, On Top/Bottom, Maximize/Minimize, App Window Switch, Quick Switch, Virtual Desktops and the `window-tools` composition root. Geometry, state-machine, mouse, COM and original dispatch tests all pass.
- [x] Each feature owns its schema/default file and edit entry. Fresh profiles use split defaults; imported legacy configs keep their effective values. Cross-file duplicate hotkeys and invalid reloads retain the old config.
- [x] Wire v3 nested menu validation/rendering and hosted id routing pass; hosted and standalone use the same module menu model.
- [x] Final-build isolated smoke: six children, occupied `Ctrl+Alt+L` and `Ctrl+Alt+Shift+G`, duplicate hosted locker exit 1 without a modal, and zero children after a hard host kill.
- [x] Hosted Window IPC on the final staged binary: a real Quick Switch config edit released Ctrl+Alt+L and occupied Ctrl+Alt+O; an invalid reload kept the previous config; restoring the file restored the original key. Suspend → Resume → Quit also passed with normal `code=0` shutdown.
- [x] The `desktop-tools-rs`, `desktop-tools-rs-ex`, `taskbar-tools-rs` and superseded `keytools-rs` source repositories have no modifications; keyboard-tools-rs contains only the already reviewed P1/P2 changes.
- [x] Staging compares host/plugin SHA-256 with the release binaries. The actual staged window plugin was wire-probed and returned all eight nested feature submenus, then exited normally on Quit.
- [x] Agent restored the final staged host as PID 28640 after standalone sign-off, using the existing keyboard profile; six children are running and no build step is pending. A corrected wire-v3 IPC harness also passed Suspend/Resume/Quit after reading the actual staged menu/status event order.

### Physical pass A — hosted Window + Keyboard

User reported the hosted behavior "基本ok" on 2026-11-07; the final Suspend Window / no-manual-Reload menu was additionally verified through real wire v3.

- [x] **Structure/config entries:** one host tray has Window before Keyboard. Window directly opens Rectangle / Resize / Mouse Drag / On Top/Bottom / Maximize/Minimize / App Window Switch / Quick Switch / Virtual Desktops. Each has its own Edit config; Resize also has Edit resize presets, and On Top/Bottom has Clear all pins. Window root has Undo and Suspend Group "Window" (no manual Reload), plus host Disable/Restart/config/log. No intermediate Window (running), duplicate plugin config entry or second Window tray.
- [x] **Rectangle edges/corners:** on a normal window, each Win+Alt+H/L/K/J cycles 1/2 → 2/3 → 1/3. Check all four Ctrl+Win+Alt+H/K/J/L corners and Ctrl+Win+Alt+C centering. Repeating an edge must not open a shell action.
- [x] **Display/maximize:** Ctrl+Win+Alt+N/P move the same window between monitors without changing its relative size; Ctrl+Win+Alt+F maximizes and restores it. With one monitor, N/P wrapping to the same monitor is N/A.
- [x] **Pin/minimize/sizer/undo:** Ctrl+Win+Alt+T pins above other windows and toggles off; Ctrl+Win+Alt+B keeps a window behind after another window is activated and toggles off; Ctrl+Win+Alt+M minimizes; Ctrl+Win+Alt+S opens Sizer and applies a preset. Ctrl+Win+Alt+U successively restores Sizer, snap, maximize and minimize states. On Top/Bottom → Clear all pins clears pins.
- [x] **Mouse drag:** hold the configured drag modifier and left-drag a normal window across a monitor boundary; repeat on a maximized window. The first movement restores it without jumping, a modifier-click without movement does not restore it, and releasing Win must not open Start.
- [x] **Switcher:** open three windows of one application. Alt+` and Alt+Shift+` cycle them forward/back without two-window ping-pong. Close one window during the sequence and continue: no error or stuck state.
- [x] **QuickSwitch:** with an Explorer folder active, open a Notepad Save As dialog and confirm it follows that folder. Ctrl+Alt+L applies the current folder again. Outside a file dialog, Ctrl+Alt+L still reaches the application (for example Double Commander list view).
- [x] **Virtual desktops:** with at least three desktops, Win+` / Win+Shift+` move next/previous; check every configured Win+Alt+number for desktops that exist, then Win+Shift+number moves the active window there without losing it or unexpectedly switching focus.
- [x] **P3 compatibility:** Win+E, Win+C, Win+1, Win+Left and Win+Shift+S remain suppressed rather than opening shell UI; Win+F2 still turns displays off and input wakes them. These are temporary P3-owned behaviors: P4 performs the exhaustive suppression matrix after moving it to keyboard-shell, and P6c moves monitor power to screen-power.
- [x] **Lifecycle/config:** Window → Suspend Group "Window" disables its hotkeys/drag while keyboard tools still work; Resume restores them. Open all eight module config entries and confirm each opens its matching TOML with the existing effective bindings/settings (the original full legacy file is unchanged). Make one harmless gap edit in Rectangle, save, and confirm auto-reload applies the new snap margin within about a second; restore and save again. Host Restart preserves Running + Disable `window-tools` with no duplicate hook or tray.

### Physical pass B — standalone Window (agent will restart between modes)

Agent stopped the host and started only the final staged standalone `window-tools.exe` as PID 3044; host count is zero.

- [x] With the host stopped by the agent, standalone `window-tools` shows the same eight module submenus and state icons, performs one snap and mouse drag, and its own Quit exits cleanly without killing other applications. User signed off on 2026-11-07.
- [x] Toggle standalone Start with Windows on/off. Agent verified that all six current/legacy Run values are absent and no host/window process remains.

P3 is signed off; P4 may proceed.

## P5 — hosted Taskbar

Automated before handoff:

- [x] `scripts\verify.ps1`: 175 tests, clippy, release and hash-verified staging.
- [x] Taskbar geometry/pin behavior tests remain green after extraction. Real wire v3 verifies three described feature submenus, `Ctrl+Alt+F9` occupancy, Suspend on/off, nested Unpin and Quit=0.
- [x] Isolated and real profiles migrate aggregate Window/Taskbar TOMLs to independent files without overwriting existing modules; active aggregates are absent and `.legacy.toml` backups retain their previous contents.
- [x] Real Explorer `--dump`, `--inspect` and `--once` return successfully.
- [x] Isolated host starts nine children; Window, GoldenDict and Taskbar hotkey probes pass; duplicate hosted locker and hard-kill cleanup still pass.

## P5 Physical — passed

Hosted pass A and standalone pass B were signed by the user. The final host was rebuilt and restarted as PID 43336 with nine plugins. Verified items:

- [x] Taskbar (running) has Center Taskbar / Hidden Taskbar / Taskbar Pin, each with description → separator → its own Edit config. There is no aggregate config entry or second tray.
- [x] Centering, Brave pin order, 100 ms polling, Ctrl+Alt+F9 and snapped-window resizing preserve the migrated values.
- [x] Suspend Group "Taskbar" pauses Center and Pin as well as the Hidden hotkey; Center restores its saved layout while suspended and Resume reapplies the configured behavior without overriding a manual Unpin all.
- [x] Agent verified hidden-state graceful Disable/Enable and hotkey recovery.
- [x] Standalone Start with Windows was toggled on/off; no Run value remains.
- [x] Standalone stable MenuId events reach Suspend, autostart, Edit and Exit; worker shutdown completes and no taskbar-tools.exe remains.
- [x] Host was restored after standalone testing.

## P6a Physical — passed

- [x] Tray tooltip and Quit item identify Screen Tools; only one Screen tray is present.
- [x] Ctrl+Alt+D draws a frame, Ctrl+Alt+S resizes it, and Ctrl+Enter captures/copies and OCRs ordinary on-screen text.
- [x] Ctrl+Alt+R starts/stops a short recording; output and existing media are intact.
- [x] Screen was stopped and the desktop host restored with nine plugins.

Automated: 116 old-workspace tests, clippy, release, Ctrl+Alt+S occupation, new-instance duplicate rejection, legacy config byte preservation, and real `screen-tools.*` window classes pass. The documented `tools/physical.ps1` does not exist and was not claimed as run.

## P6b Screen hosted baseline

Automated: 290 passed / 11 ignored workspace tests, clippy, release/staging and ten-plugin smoke pass. All original Screen tests moved with Record (6), Capture (5), OCR (23) and Draw (37). Real wire-v3 verifies four described feature submenus, hotkey ownership, Suspend/Resume and clean Quit; a real staged frame/capture/OCR/result cycle also passes. Split-config probes verify legacy migration, Draw hotkey replacement, same-process reload and atomic rollback on invalid OCR TOML. The real profile preserves every effective legacy value and a byte-identical `.legacy.toml` backup. Multi-monitor and trailing-Draw red/green probes remain green. Hosted delivery used PID 21340 with ten children.

- [x] Hosted physical pass accepted four alphabetical feature submenus and their separate config entries. Logs confirm draw, resize, capture/clipboard/OCR, recording, Pin, group Suspend and a clean restart; no modal error appeared.
- [x] On the secondary monitor, both pointer drawing and Ctrl+Alt+W place the Frame there; the drawn Frame remains visible after mouse-up instead of reopening Draw, and moving/resizing across the monitor boundary is not clamped back to the primary display.

## P6b Physical pass B (standalone Screen, delivered PID 20976) — passed

The host and other plugins are intentionally stopped. The staged standalone Screen process owns `Ctrl+Alt+D` and reads the same split profile files.

- [x] The single Screen Tools tray shows Capture / Draw / OCR / Record with descriptions, separators and Edit config entries, plus Screen actions, `Suspend Group "Screen"` and `Exit screen-tools`.
- [x] Click `Edit Draw config`; it must open `screen-draw.toml`, proving the standalone native-menu event path rather than only the hosted IPC path.
- [x] Toggle Suspend on/off: its checked state changes, `Ctrl+Alt+D` is released while checked and works again after resume; draw one frame after resume.
- [x] Click `Exit screen-tools`; the agent verified PID 20976 and every other standalone Screen process exited, then restored hosted PID 25072 with ten plugins. P6b is signed off and P6c may start.

## P6c Physical — Power transfer (signed off; host PID 23220)

- [x] Screen contains **Power** between OCR and Record, with a grey description, separator and `Edit Power config`; it opens `screen-power.toml` containing the existing `win+f2` binding.
- [x] Press `Win+F2`: every monitor turns off; ordinary input does not immediately turn it back on during forced-off mode. Press `Win+F2` again to wake and clear that mode. No Frame is created by this hotkey.
- [x] Suspend Group "Screen" releases `Win+F2`; Resume reclaims it. Window's Undo binding and other Screen features remain independently usable.
- [x] Agent verified the final binaries: one-time custom-binding import leaves `window-actions.toml` byte-identical, Screen owns the migrated hotkey, Window releases it, both plugins Quit with code 0, and ten-plugin smoke passes.

## P6c Physical — Scale transfer (passed; final host PID 22288)

- [x] Window → Resize contains separate **Edit Resize config**, **Edit Scale config** and **Edit resize presets** entries. Scale opens `window-scale.toml` with the existing `ctrl+alt+z` binding; Resize still opens its unchanged file.
- [x] Focus a normal window and press `Ctrl+Alt+Z`: a fullscreen live preview of that window appears. Press the hotkey again (or left-click) to dismiss it; ordinary source-window contents remain visible in the preview.
- [x] With a Screen Frame active, `Ctrl+Alt+Z` zooms the application under the cursor, not the Frame itself. The preview must not contain a fullscreen cyan Frame border; dismissing Scale leaves the original Frame visible and unchanged.
- [x] Suspend Group "Window" releases Scale; Resume restores it. Screen hotkeys and Window Resize/Sizer remain independently usable.
- [x] Agent verified the final binaries: Screen releases the old action/hotkey; Window imports a custom binding without rewriting Resize or Screen actions; the real DWM overlay toggles on/off, Quit with an active overlay exits with code 0 and leaves no overlay, and both plugins Quit cleanly. Full workspace tests, clippy, OCR and ten-plugin smoke pass.
- [x] Agent reproduced the Frame regression as a 1440-pixel cyan preview border, then verified the fix with real Draw + Scale input: the fullscreen border is gone, the original Frame stays visible at its original rectangle after Scale closes, and both plugins Quit with code 0.

## P6c Physical — Flameshot handoff: passed

- [x] Screen contains **Flameshot Handoff** after the five native feature submenus. **Edit Flameshot config** opens `screen-flameshot.toml`; no Screen actions config entry remains.
- [x] With Flameshot configured, drawing a Frame and pressing `Ctrl+F` opens it on the current Frame region without a second desktop-tools host or tray. Physical acceptance signed off.
- [x] Agent verified the final staged binaries: six wire submenus and separate config entries; one-time path import leaves `screen-actions.toml` unchanged; a missing executable logs the failure and restores the original Frame; Suspend/Resume/Quit and ten-plugin smoke pass. Full verification: 292 passed, 11 ignored, clippy and SHA-checked staging green.

## P6c Physical follow-up — action exclusion during Flameshot: passed

- [x] Agent verified with the real external tool: Screen releases the configured global Ctrl+T during handoff and restores it afterward, including across Suspend/Resume and config reload; Power remains registered. Global Draw opens no overlay, posted Ctrl+A and duplicate Ctrl+F cannot mutate the hidden Frame or start another session, a second handoff reuses the daemon, and Draw works after exit. Clean hosted Quit, 294 tests / 11 ignored, clippy, staging and ten-plugin smoke pass.
- [x] While Flameshot is open, press `Ctrl+Alt+D`: no new Draw overlay appears. Flameshot's own `Ctrl+T` still pins through Flameshot, not the hidden Frame.
- [x] Exit Flameshot and press `Ctrl+Alt+D`: Draw works immediately.
- [x] Complete two separate Frame → Flameshot handoffs: no additional Flameshot daemon/tray instance appears; each GUI session opens and closes normally.
- [x] Recording guard verified with real FFmpeg and Flameshot: during Frame recording, `Ctrl+F` was rejected and did not launch Flameshot; normal stop produced 31 decodable frames. The complete handoff/mutex regression and clean Quit passed afterward.

## P7 Physical — App Toggle: signed off

- [x] Window's final external submenu is **App Toggle**. **Edit App Toggle config** opens `window-app-toggle.toml`; all native Window modules remain before it.
- [x] With an ordinary application focused, press `Win+E`: Double Commander launches or receives focus and Explorer never opens. Press again to return to the previous application; press a third time to restore Double Commander rather than launch a duplicate.
- [x] Hold `Win+E`: the window must not oscillate. Suspend Group "Window" disables the toggle; Resume restores it without changing other Shell Keys suppressions.
- [x] With ``Ctrl+` `` unoccupied, repeat the focus/return/held-key checks for WezTerm. External tools must also leave the existing ``Alt+` `` and ``Alt+Shift+` `` App Window Switch bindings unoccupied. The final binary's real WezTerm focus/return/Quit path passed using isolated ``Ctrl+Alt+` `` before CopyQ released the default.
- [x] Agent checks: canonical full paths handle Scoop aliases; real WezTerm toggle returns focus and restores minimized targets; clean hosted Quit, 304 tests, clippy, release/staging and ten-plugin smoke pass.


## P8 Physical — swapWindows: passed

- [x] Agent verified an isolated `Ctrl+Alt+Shift+X` binding with real windows: visible slots exchange, B keeps focus, one Undo restores both rectangles, bare Esc cancels, the 15-second timeout expires, Reload and Suspend release Esc, Resume and clean Quit work. Unit tests also cover same-window, invalid, minimized/maximized, per-window DWM insets and grouped Undo.
- [x] User approved `Ctrl+Win+Alt+X` as the default; add only that binding to the current profile, without changing existing Rectangle bindings.
- [x] After enabling it, select A, then B: their slots exchange and B stays focused; one Window Undo restores both. Repeat across monitors with different DPIs.
- [x] Re-select the same window, press bare Esc, wait for timeout, and try a minimized or maximized B: each cancels/rejects without moving either window; bare Esc is not captured after cancellation or Suspend.
- [x] Hold the swap hotkey: it must not repeatedly arm/cancel; ordinary repeatable Rectangle shortcuts must still auto-repeat.

## P9a Physical — shared executable and tray icons: passed

- [x] Agent checks: the shared renderer covers all 11 executable assets and all states; ICO files contain 16/20/24/32/48/256-pixel entries. Full tests, Clippy, release/staging and ten-plugin smoke pass. Windows extracted large and small icons from all 11 staged EXEs; Screen exposes resources 1–4.
- [x] Hosted Desktop Tools shows one blue `D` tray icon; no plugin creates a second tray icon. Window/Screen/Taskbar menus and hotkeys still work.
- [x] Standalone Window shows a blue `W`; Suspend turns it grey and Resume restores it. Error/reload badges are covered by renderer/resource tests.
- [x] Standalone Screen shows a teal `S`; Suspend greys it and Resume restores it. Its Frame continues to use embedded resource 1.
- [x] Standalone Taskbar shows a light-blue `T`; standalone Shell Keys shows a purple `S`. Suspend/Resume grey/restore each icon without changing menus.
- [x] In Explorer, staged EXEs keep recognizable letters and colors in large and small icon views, with no missing/default icon.

## P9b — stable profile and distribution package

- [x] Agent copied the live profile to `%LOCALAPPDATA%\desktop-tools\Profile\User` without moving or changing the old repository copy; all 45 hashes match, all ten children use the new path, and `DTTOOLS_PROFILE` is persisted for future autostart. No old TOML was rewritten during the switch.
- [x] User physically confirmed a feature **Edit config** entry opens the new `%LOCALAPPDATA%\desktop-tools\Profile\User` file and that ordinary hotkeys and menus behave as before on the Scoop-hosted runtime.
- [x] `scripts/package.ps1` created `desktop-tools-0.1.0-win64.zip` and SHA-256; the archive contains host plus ten plugin exe/manifest pairs and no profile.
- [x] Agent extracted the zip, started it with an empty isolated profile, and observed all ten plugins running with the fresh defaults. The real host was then restored against the stable profile.
- [x] Agent added CI and tag-release workflows using the same verify/package scripts; both YAML files parse. Host `--version` prints `0.1.0`, and enabling host autostart now clears the known old host and standalone Window/Taskbar Run values in addition to the previous legacy names.
- [x] Agent stopped the duplicate legacy AHK KeyboardSwitch and KeyboardAutoSwitchBack processes and moved their two Startup shortcuts to `%LOCALAPPDATA%\desktop-tools\legacy-startup-backup`; neither legacy process nor either Startup link remains, and the new suite retains its probed hotkeys.
- [x] Real CI run 43684828643 and Release run 43684838754 passed. The downloaded v0.1.0 zip is 6,022,648 bytes and matches both its SHA-256 file and GitHub's API digest. Scoop manifest commit `fde211e` is published; installation from `pile/desktop-tools`, Start Menu shortcut creation, `--version`, ten-plugin startup from the Scoop path and hotkey ownership all pass. The current host is PID 26796. Uninstall/reinstall retained the persist marker; it was then removed. The later v0.2.0 real-reboot gate confirmed one Scoop-autostarted host, ten hosted plugins and no legacy duplicates. The user will test the local Scoop manifest installation; failed-plugin rollback is not a requested gate.

## P10 — multi-step GoldenDict hotkeys and required regressions

- [x] Kit sequence tests pass for double-press, 400 ms timeout, three-step and shared-prefix candidates, conflict/plain-key rejection, unique registration and reset. Every sequence step requires Ctrl, Alt or Win.
- [x] A real final-binary hosted probe with an isolated profile and fake GoldenDict executable passed two-step, three-step/shared-prefix, legacy `!xx`, ordinary single-press and clean Quit checks with correct group and clipboard arguments. The real dictionary and current bindings were untouched.
- [x] Full format/workspace tests/Clippy/release/staging and ten-plugin smoke pass. Paused default-config semantic edits were withdrawn; no Taskbar code was changed.
- [x] Real reboot gate passed: Windows booted at 20:18:39 and Scoop autostart created one new host, PID 16380 at 20:19:33, with ten hosted plugins using `current\config`; no legacy AHK or standalone Taskbar duplicate appeared. The sole Run entry targets Scoop `current`, all 34 TOML files match the rollback copy, and four representative hotkeys remain owned.

## P10 follow-up — Quick Switch activation and Shell Keys reference (signed off)

- [x] Quick Switch automatically navigates a file dialog once per activation, with no automatic retry after success or failure. Leaving and returning to the dialog permits one new attempt; the manual `fileDialog` binding is unchanged. A real Save As dialog reproduced four initial/eight total attempts on the old release, then measured one initial/two total attempts on the new binary, with no periodic retry while focus stayed put.
- [x] All 66 default Shell Keys entries have inline Windows-function descriptions. A parser test requires every entry to be documented; the live profile received comments only, with TOML semantic equality and unchanged binding order verified before replacement.
- [x] Physical pass: leave a Quick Switch-compatible file dialog open and confirm its folder/address field is not repeatedly refreshed. Switch to another window and back: it refreshes once more. Open **Shell Keys → Edit config** and confirm the inline Windows-function reference is visible.

## P10 follow-up — Windows standard and portable profile discovery

- [x] Agent verified the final binaries in five isolated launch modes: ordinary `%APPDATA%` fallback, existing exe-adjacent portable `Profile/User`, explicit relative `--profile`, `DTTOOLS_PROFILE` precedence, and legacy `KBTOOLS_PROFILE` fallback after an empty `DTTOOLS_PROFILE`. Ordinary and portable full installations each passed one absolute profile to all ten children and exited cleanly.
- [x] Full workspace tests, Clippy, release/staging and ten-plugin smoke pass. The current staged host is PID 19584 with ten children; its explicit `%LOCALAPPDATA%` profile remains authoritative and all 34 real TOML files stayed byte-identical. No migration to Scoop persist or environment-variable removal has occurred.
- [x] Physical regression passed: a feature's **Edit config** still opens the existing `%LOCALAPPDATA%\desktop-tools\Profile\User` file, and an ordinary hotkey plus the menu/tray behavior remain unchanged.

## Post-P10 — Open Profile Folder and Scoop profile migration (current host PID 2316)

- [x] Agent verified full fmt/test/Clippy/release/staging and ten-plugin smoke.
- [x] Agent copied 45 files to `C:\Users\User\Scoop\persist\desktop-tools\Profile\User`; all 34 TOML files match the retained source byte-for-byte. The old target's test marker was backed up before removal.
- [x] Scoop manifest commit `9210bb8` adds only `"persist": "Profile/User"`. Scoop without profile environment variables passed ten-plugin startup, child-path and hotkey checks. The staged host and its ten children now use the same persist directory; `DTTOOLS_PROFILE` was repointed there.
- [x] Superseded by the completed `config` migration below; use the Post-v0.2.0
      physical checklist for the current native-menu click.

## Config directory migration

- [x] Resolver tests and six real isolated launch cases pass: fresh standard/portable `config`, legacy portable/standard `Profile/User`, and new-over-legacy precedence when both exist. Every case started ten plugins with one absolute profile and exited cleanly.
- [x] Full fmt/workspace tests/Clippy/release/staging pass. Existing explicit `--profile` and environment-variable behavior is unchanged; no current profile was rewritten by resolver tests.
- [x] v0.2.0 CI Release run 44882940399 passed in 6m8s. Its 6,051,706-byte zip matches the published SHA-256 file and GitHub API digest. Scoop manifest `baea89d` persists `config`.
- [x] Agent copied 45 files to `persist\desktop-tools\config`; all 34 TOML files match the retained legacy profile byte-for-byte. Real `scoop update` upgraded 0.1.0 to 0.2.0, verified the asset hash and created the config persist link. Scoop started ten plugins without profile environment variables and passed hotkey checks.
- [x] Final runtime is Scoop v0.2.0 host PID 20764 with ten children, all using `current\config`. Both persistent profile variables are absent; four representative hotkeys remain owned and the old profile is retained unchanged.

## Post-v0.2.0 Physical — shortened config: signed off

- [x] The tray still has one host icon and the same feature groups. **Open Profile
      Folder** opens `C:\Users\User\Scoop\apps\desktop-tools\current\config`,
      the stable Scoop link to `persist\desktop-tools\config`.
- [x] Open an existing TOML from that folder: personal bindings, Shell Keys
      comments and other settings are still present; there is no second host or
      duplicate plugin tray icon.
- [x] Use a common hotkey such as `Ctrl+Alt+D`: Draw, its menu and the rest of
      the suite behave as before the directory move.

## P11 — virtual-desktop Pin isolation

- [x] Full fmt/workspace tests/Clippy/release/staging and ten-plugin smoke pass.
      A real final-binary probe passed separate Pins on two desktops, both switch
      directions, first Pin on a fresh desktop without a previously visible Frame,
      desktop close and subsequent Frame deduplication without losing the Pin.
- [ ] Physically create a Pin on each of two desktops. Each appears only on its
      own desktop; switching back restores its position and image. Pins add no
      taskbar or Alt+Tab entry, and drag, edge-resize, Esc/right-click still work.
- [ ] On more than one monitor/DPI, confirm a Pin keeps its original geometry and
      appearance. Close a desktop containing a Pin and confirm it remains usable
      on the surviving desktop.

## P12 — layered configuration, first slice

- [x] Shared overlay tests pass for nested tables, scalar replacement, whole-array
      replacement and explicit empty-array clearing. Existing full files keep the
      same effective values; the sparse user file is never rewritten and the
      separate `defaults/keyboard-switch.toml` reference tracks the shipped default.
- [x] Keyboard Switch accepts an empty sparse file, leaves CapsLock and switching
      unbound by default, rejects uninstalled declared layouts, and does not invent
      US/Chinese layouts. Sparse menu toggles write only their explicit setting.
- [x] Full fmt/workspace tests/Clippy/release/staging and ten-plugin smoke pass.
      The final staged host has ten children on the existing Scoop config; its full
      `keyboard-switch.toml` is byte-identical to the rollback copy and the generated
      default reference matches the repository default.

## P13 — safe defaults and unavailable-target key handling

- [x] Screen unit tests restrict pass-through to Frame-only global actions and
      reject duplicate gate hotkeys. A real foreground-window probe verified that
      `Ctrl+Enter` arrives exactly once with Ctrl held, both for an instant chord
      and a 0.2-second hold; the hold path creates no synthetic Ctrl release.
- [x] An isolated hosted Window probe with missing Double Commander/WezTerm paths
      skipped both bindings, logged both warnings, allowed the ordinary hotkey to
      be registered by another process, and exited cleanly. Pure configuration
      parsing remains independent of installed machine paths.
- [x] GoldenDict Auto-search defaults have empty group/search type in the correct
      `[goldendict]` section; tests verify that empty means no group argument.
      Existing user groups and bindings were not changed.
- [x] Full fmt/workspace tests/Clippy/release/staging and ten-plugin smoke pass.
      The complete Flameshot handoff, recording guard and daemon-reuse probe passed;
      normal recording stop produced 30 decodable frames. All 34 user TOML files
      remain byte-identical to the rollback copy; only the generated Keyboard Switch
      reference is additional.
- [ ] With no visible Screen Frame, press `Ctrl+Enter` in an application that uses
      it: the application receives it once. With a visible Frame, Capture still works.
- [ ] Confirm `Win+E` and ``Ctrl+` `` still toggle the installed Double Commander
      and WezTerm targets, and Auto-search retains the existing personal groups.

## P16 — Pin actions, editor pairs and active naming

- [ ] Focus a Pin and press `Ctrl+C`: CopyQ contains the original image, and both
      the Pin and its Frame close. A deliberate clipboard failure must leave the Pin.
- [ ] `Ctrl+S` opens Save As and `Ctrl+Shift+S` quick-saves under
      `Pictures\screen-tools`; compare PNG dimensions with the original capture.
      Both paths retain the Pin, and cancelling Save As also retains it.
- [ ] Make a normal Capture and short recording: new files use
      `Pictures\screen-tools\screen-tools-*` and `Videos\screen-tools\screen-tools-*`.
      Old `screen-tools-rs` files remain, and Task Manager shows `screen-tools`.
- [ ] Open GoldenDict Search and Auto-search configs: comments say to create groups
      in GoldenDict first; the live groups are `default` / `translate`, with no
      `grammar`. Confirm both existing search hotkeys and Auto-search still work.
- [ ] Set `[editor] command` in `desktop-tools.toml`, then use Edit config: one
      editor process opens the default reference first and user file second.
      Check a hosted plugin, a Window/Screen feature, Edit core.toml and Edit
      desktop-tools.toml. Reference snapshots must not be presented as layered for
      Window, Screen, Taskbar, Key Map or Shell Keys.
- [ ] `ctrl+shift+alt+g` still toggles Auto-search after its user file became sparse.
      AHK `^!+g` and `!xx` are intentionally unsupported, not silent fallbacks.
