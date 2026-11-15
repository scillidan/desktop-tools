# desktop-tools agent notes

Read `MERGE.md` for architecture/migration decisions and `CHECKLIST.md` for the
current acceptance gate before changing a delivered workflow.

## Build and verification

- Use `scripts/verify.ps1` for fmt, tests, clippy, release and staging. It forces
  the repo-local `target`; the machine's global Cargo target can contain stale
  binaries and must not feed staging.
- Use `scripts/package.ps1` for a distribution zip. It runs the same verification
  and includes no user profile.
- Stage and run the host and all plugins from one build. IPC wire v3 is not
  compatible with mixed old/new binaries.
- Restart the actual host/plugins after a release build and verify the running
  executable paths before asking for physical acceptance.

## Runtime boundaries

- The suite has one host/tray and ten plugins. Groups own code, configuration and
  menus; they do not each imply another process.
- Configuration belongs to feature TOMLs. Reload is atomic across shared hotkey
  namespaces. Import a legacy file once only when its destination is absent.
- The active profile is selected by `--profile`, then `DTTOOLS_PROFILE`, then the
  executable's `Profile/User`. Preserve old profiles and migration sources.
- Suspend is temporary; Disable is persistent and requires graceful cleanup.
  Hosted and standalone menus share the same command/menu model.

## Acceptance

- Unit/IPC probes do not replace physical tray clicks, global hotkeys, OCR,
  recording, multi-monitor/DPI or external-application checks.
- Low-level hooks intentionally ignore injected input. A registered hotkey also
  needs a functional-result assertion, not just an ownership probe.
- Record unverified items explicitly; do not sign off an integration whose
  external application is absent.
