# Calumma v2

**Your personal whiteboard** — bounded canvases you draw on with a pen, shapes, and text.
Layers, masks, and titlebar project tabs keep work organized; everything persists locally.

Desktop app: Rust + Slint shell, Rust/wgpu engine, SQLite storage. Targets macOS 26,
Windows 11, and current Linux.

<p align="center">
  <img src="design/example/landing.png" alt="Calumma landing — new project screen" width="720">
</p>

<p align="center">
  <img src="design/example/editor.png" alt="Calumma editor — layers, tools, and canvas" width="720">
</p>

Read **`AGENTS.md`**, **`docs/FLOW.md`**, and **`docs/STYLE.md`**.

## Quick start

```bash
./manage.py examples # optimize design/example/*.png (drop sources in design/example/source/)
./manage.py dev      # build and run the GUI shell
./manage.py gui-check # compile-check without opening a window
```

Landing → create/preset/recent → editor with tools panel and board embed.

## Layout

| Path | Role |
| --- | --- |
| `design/` | visual tokens, SVG icons, README screenshots (`design/example/`) |
| `docs/` | all prose docs — `FLOW.md`, `STYLE.md`, `ENGINE.md`, `RENDERING.md` |
| `translations/` | UI locale JSON (`en` today) |
| `engine/` | Cargo workspace (`Cargo.toml`, rustfmt, clippy) + crates |
| `engine/core` | document, tiles, camera, history, shapes |
| `engine/io` | SQLite projects |
| `engine/ops` | AI/image op registry (Cut BG shipped; see AGENTS) |
| `engine/render` | wgpu (surface from shell) |
| `engine/app` | Rust API (`Engine`, `NativeSurface`) for the GUI shell |
| `engine/ffi` | Real `Engine`/`Inner` implementation. No C ABI any more (`crate-type = ["rlib"]` only) — `calumma-app` re-exports it as plain Rust |
| `gui/` | Slint desktop shell (`calumma-gui` binary) |
| `manage.py` | Python 3.14 task runner |
| `cli/` | Python helpers + leaf tools used by `manage.py` |

## Install

There is no packaged release yet — build from source (`./manage.py dev` / `build` above).
CI (`.github/workflows/main.yml`) currently only lints, security-scans, and runs the engine's
test suite; a per-platform `gui/` release pipeline is open work.

## Notes

- Projects: OS-native app-data dir + `Calumma/calumma.sqlite` (`ProjectStore::default_path`,
  via `dirs` — never a hardcoded path).
- The legacy Swift shell lives in `legacy-macos-shell/` (gitignored reference) — do not
  extend it. The earlier Qt shell (`ui/`) has been removed entirely, not archived.
