# Miw

**Your personal whiteboard** — bounded canvases you draw on with a pen, shapes, and text.
Layers, masks, and titlebar project tabs keep work organized; everything persists locally.

Desktop app: Rust + Slint shell, powered by the **Calumma** engine (Rust/wgpu + SQLite).
Targets macOS 26, Windows 11, and current Linux.

<p align="center">
  <img src="design/example/landing.png" alt="Miw landing — new project screen" width="720">
</p>

<p align="center">
  <img src="design/example/editor.png" alt="Miw editor — layers, tools, and canvas" width="720">
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
| `engine/` | Calumma engine — Cargo workspace (`Cargo.toml`, rustfmt, clippy) + crates |
| `engine/core` | document, tiles, camera, history, shapes |
| `engine/io` | SQLite projects |
| `engine/ops` | AI/image op registry (Cut BG shipped; see AGENTS) |
| `engine/render` | wgpu (surface from shell) |
| `engine/app` | Rust API (`Engine`, `NativeSurface`) for the GUI shell |
| `engine/ffi` | Real `Engine`/`Inner` implementation. No C ABI any more (`crate-type = ["rlib"]` only) — `calumma-app` re-exports it as plain Rust |
| `gui/` | Miw desktop shell — Slint UI (`miw` package, `Miw` binary) |
| `manage.py` | Python 3.14 task runner |
| `cli/` | Python helpers + leaf tools used by `manage.py` |

## Install

GitHub Releases ships an installer for each OS whenever the workspace version in
`engine/Cargo.toml` is bumped on `main` (must match `gui/Cargo.toml`):

- **macOS:** `Miw-<version>.dmg` — ad-hoc signed, not notarized. Right-click → Open the first
  time, or `xattr -dr com.apple.quarantine /Applications/Miw.app`.
- **Windows:** `Miw-<version>-windows-x86_64.msi` — run the installer. Windows 11, 64-bit.
- **Linux:** `Miw-<version>-linux-amd64.deb` for Debian/Ubuntu, or
  `Miw-<version>-linux-x86_64.tar.gz` elsewhere. Needs Vulkan and X11 (or XWayland).

```bash
./manage.py package   # host-native installer into dist/
./manage.py dev       # or run from source
./manage.py build
```

CI (`.github/workflows/main.yml`) lints, security-scans, then tests and packages on Linux
first and on Windows and macOS only once Linux passes. All three installers are published as
a GitHub release on a version bump.

## Notes

- Projects: OS-native app-data dir + `Miw/miw.sqlite` (`ProjectStore::default_path`,
  via `dirs` — never a hardcoded path).
- The legacy Swift shell lives in `legacy-macos-shell/` (gitignored reference) — do not
  extend it. The earlier Qt shell (`ui/`) has been removed entirely, not archived.
