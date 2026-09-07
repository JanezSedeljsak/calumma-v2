# gui/

**Desktop shell** for Calumma — Rust + [Slint](https://slint.dev) UI for macOS 26, Windows 11,
and current Linux. Targets modern OS releases only; no legacy shell support.

The legacy Swift reference is in `legacy-macos-shell/` (gitignored). Migration plan:
[`docs/plans/02-slint-shell.md`](../docs/plans/02-slint-shell.md).

## Run

From the repo root (so `design/` and `translations/` resolve):

```bash
./manage.py dev
```

Or:

```bash
./manage.py gui-check   # compile check
./manage.py build       # release build
cargo run --manifest-path gui/Cargo.toml
```

## Layout

| Path | Role |
| --- | --- |
| `src/shell/` | Prefs, theme, l10n, controller (no engine logic) |
| `src/board/` | wgpu surface embed (macOS Metal child layer today) |
| `src/input/` | Keyboard routing |
| `src/ui_bridge.rs` | Slint property sync |
| `ui/*.slint` | Declarative UI (`calm/*`, landing, editor, modals) |

Engine boundary: `calumma-app` only — see `engine/app/`.
