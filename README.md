# Calumma v2

Native rewrite: SwiftUI shell + Rust/wgpu engine. Multi-project whiteboard with
SQLite persistence and shared design tokens (not CSS).

Read **`AGENTS.md`**, **`FLOW.md`**, and **`design/STYLE.md`**.

## Quick start

```bash
./manage.py tokens   # regenerate Swift tokens from design/tokens.json
./manage.py dev      # build ffi + open Xcode
```

Run the **Calumma** scheme. Landing → create/preset/recent → editor with top tabs.

## Layout

| Path | Role |
| --- | --- |
| `design/` | visual tokens, STYLE, SVG icons |
| `FLOW.md` | product flow: screens, shortcuts, import/export |
| `translations/` | UI locale JSON (`en` today) |
| `engine/` | Cargo workspace (`Cargo.toml`, rustfmt, clippy) + crates |
| `engine/core` | document, tiles, camera, history, shapes |
| `engine/io` | SQLite projects |
| `engine/ops` | AI/image op registry (Cut BG shipped; see AGENTS) |
| `engine/render` | wgpu (surface from shell) |
| `engine/ffi` | C ABI for Swift |
| `platform/macos` | SwiftUI shell + `.swift-format` |
| `manage.py` | Python 3.14 task runner |
| `cli/` | Python helpers + leaf tools used by `manage.py` |

## Notes

- Projects: `~/Library/Application Support/Calumma/calumma.sqlite`
- Tab switch clean-loads from DB (no preload)
- Shell knobs only; canvas/state in Rust
- Custom icons only; no icon libraries
