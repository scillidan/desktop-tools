# desktop-tools (merge workspace)

Status: planning only. This is the future home of the merged desktop suite;
no product code lives here yet. The existing products stay in their own repos
until the merge gate in [`MERGE.md`](MERGE.md) is met.

- [`MERGE.md`](MERGE.md) — the full merge analysis: plugin runtime, IPC/menu
  contract, unified tray, profile layout, packaging, hot-plug, per-product
  notes, repo strategy and the re-review gate.

The current products:

- `../keyboard-tools-rs/` — keyboard/layout/GoldenDict suite (host + plugins)
- `../desktop-tools-rs/` — window/desktop hotkeys, including `toggleScreen`
- `../taskbar-tools-rs/` — Explorer taskbar tools
- `../desktop-tools-rs-ex/` — OCR/capture/recording (optional plugin later)

Do not copy code into this repo until the gate is explicitly reopened.
