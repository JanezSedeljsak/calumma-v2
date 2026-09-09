# gui/

**Miw desktop shell** — Rust + [Slint](https://slint.dev) UI for macOS 26, Windows 11,
and current Linux. Powered by the Calumma engine (`calumma-app`).

## Run

From the repo root:

```bash
./manage.py dev       # build and run
./manage.py gui-check # compile-check only
./manage.py build     # release binary
```

## Layout

| Path | Role |
| --- | --- |
| `ui/` | Slint screens and calm components |
| `src/` | Rust — shell controller, board host, input, theme bridge |
| `Cargo.toml` | `miw` package, `Miw` binary |

Engine boundary: `calumma-app` only — see `engine/app/`.
