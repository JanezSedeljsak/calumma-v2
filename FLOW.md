# FLOW.md — how Calumma works

Base product documentation: screens, what you can do, navigation, canvas interaction,
shortcuts, persistence, and import/export. Companion to `AGENTS.md` (architecture) and
`design/STYLE.md` (look). Platform shells implement chrome and key bindings; the engine
owns the board.

**Design intent:** clean, minimalistic UI — brand, tools, and the paper. No dashboard
chrome. Interaction density should grow toward Photoshop / Krita / Figma / GIMP without
looking like a control panel.

**Shortcuts:** mimic Photoshop (and common macOS drawing-app habits) wherever they map
cleanly. Platform-specific chords live in the shell; document them here when added.

---

## Screens

```
Landing ──create / open recent──► Editor
   ▲                                 │
   └──────── close last tab ─────────┘
                    │
                    └── Settings (sheet)
```

| Screen | Purpose |
| --- | --- |
| **Landing** | Create a project (name + resolution / presets) or open a recent from SQLite |
| **Editor** | Draw on one board; titlebar project tabs; floating tool + layers islands |
| **Settings** | Theme + language (shell knobs only) |

There is no separate “project browser” beyond Landing recents. Closing the last editor tab
returns to Landing.

---

## Landing

1. Enter a project name (default from i18n).
2. Set width × height, or pick a **preset** from tokens (product data, not i18n).
3. **Create** → engine creates SQLite project, opens it, fits camera, shows Editor.
4. **Recents** → open existing project (full load).
5. Settings gear → theme / language sheet.

Hero pane is decorative artwork only — no second create form.

---

## Editor layout

```
┌─ ● ● ●  [tab] [tab] [+] ──────────────────── ⚙ ─────────┐
│                                                         │
│  ┌────┐                                    ┌─────────┐  │
│  │pen │                                    │ layers  │  │
│  │shp │         Metal board (full bleed)   │  list   │  │
│  │…   │         floating islands overlay   │         │  │
│  │zoom│                                    └─────────┘  │
│  └────┘                                                 │
└─────────────────────────────────────────────────────────┘
```

- **Tabs:** in the window titlebar (right of traffic lights); switch = save → close → open selected.
- **Tools (shell knobs):** pen + one shape control (popover: line / rect / ellipse / arrow); colour; brush; fill when shape active; zoom slider.
- **Board:** viewport-sized Metal surface over a subtle rotated desk grid (island/surface colour); paper is a white filled vector layer.
- **Layers:** floating island; first layer is **Paper** (white vector rect, toggle / delete); add / select / visibility / delete; hover shows thumbnail popover + shader outline; Clear / Cut BG when available.
- **Zoom:** canvas-relative range — zoom out until the paper fills ~50% of the viewport; zoom in up to 10× that floor (also capped so ~400 doc px still span the short viewport side). Slider is logarithmic so fine control near the close end.

---

## Drawing and canvas

| Action | How |
| --- | --- |
| Paint / place shape | Click-drag on the board (pointer down → move → up). Engine converts **screen** coords. |
| Live preview | GPU stroke/shape while dragging; CPU commit into sparse tiles on pointer-up. |
| Pan | Scroll wheel / trackpad scroll; Space-drag; or Option/⌘-drag |
| Zoom | Pinch; ⌘ + scroll; Option + scroll; or ⌘`=` / ⌘`-` |
| Fit to view | `0` or Board menu — comfortable fit; zoom-out floor keeps paper ~50% of viewport |
| Space-pan | Hold Space for temporary hand tool (Photoshop-style) |

Camera clamping, zoom floor, and dirty-flag render live in Rust — never reimplemented in
Swift.

---

## Projects and persistence

| Concern | Behaviour |
| --- | --- |
| Store | `~/Library/Application Support/Calumma/calumma.sqlite` |
| Autosave / explicit save | Engine dirty flag + `⌘S`; tab switch and close save first |
| One board per project | Bounded document size chosen at create time |
| Export image / PDF | **Not shipped** (deferred) |
| Import image / PSD | **Not shipped** (deferred) |
| Clipboard paste artwork | Landing copy hints at it; full import path deferred |

When import/export land, document formats and menus here; keep encode/decode in
`engine/io` and file dialogs in the platform shell.

---

## Layers and ops

- Raster layers (sparse 256×256 tiles); optional non-destructive mask.
- Add layer: `⌘⇧N` (shell).
- Clear active layer: `⌘⌫`.
- **Cut BG** (Remove Background): macOS Vision via `calm_engine_run_op` when available.
  Shell never mutates the stack after the op. Details: `AGENTS.md` → AI ops.

---

## Shortcuts (macOS today)

Align new bindings with Photoshop where possible. Engine actions go through FFI; tool /
panel toggles are shell knobs.

### Global / menu

| Shortcut | Action | Photoshop-ish? |
| --- | --- | --- |
| `⌘N` | New project (Landing) | New document |
| `⌘S` | Save | Yes |
| `⌘Z` | Undo | Yes |
| `⌘⇧Z` | Redo | Yes (Ps redo varies by platform; we use ⌘⇧Z) |
| `⌘,` | Settings | macOS prefs |
| `⌘T` | Toggle theme | Calumma-specific |
| `⌘⌥L` | Toggle layers panel | Close to Ps panels |
| `0` | Fit to view | Ps `⌘0` is fit; bare `0` is our fit today |

### Tools and brush

| Shortcut | Action | Photoshop-ish? |
| --- | --- | --- |
| `P` | Pen | Brush is `B` in Ps — prefer `B` when expanding brush family |
| `L` | Line | Close to line/shape tools |
| `R` | Rectangle | Ps rectangle is often `U` (shape); `R` is fine for now |
| `O` | Ellipse | Ps ellipse under shape (`U`) |
| `A` | Arrow | Calumma-specific |
| `F` | Toggle shape fill | — |
| `[` / `]` | Brush smaller / larger | Yes |

### Layers / view

| Shortcut | Action |
| --- | --- |
| `⌘⇧N` | Add layer |
| `⌘⌫` | Clear active layer |
| `⌘=` / `⌘+` | Zoom in |
| `⌘-` | Zoom out |

### Pointer modifiers (board)

| Gesture | Action |
| --- | --- |
| Drag | Paint / shape |
| Space (hold) | Temporary pan (hand cursor) |
| Space-drag / Option-drag / ⌘-drag | Pan |
| Scroll | Pan |
| ⌘ + scroll or Option + scroll | Zoom toward cursor |
| Pinch | Zoom |

Cursors: crosshair on the board, open/closed hand while space-panning, zoom-in while ⌘/Option
held over the board, pointing hand on chrome controls.

---

## Shortcut policy for agents

1. Prefer Photoshop / industry defaults when adding shortcuts (`B` brush, `E` eraser,
   `V` move, `Space` temporary pan, `⌘0` fit, `⌘1` 100%, etc.).
2. Keep bindings in the **platform** shell (`CalummaApp` commands + editor key catcher).
   Document every user-facing chord in this file in the same change.
3. Do not invent conflicting chords for engine vs chrome; one map, one place to look.
4. Windows / future shells: same *actions*, OS-native modifiers (`Ctrl` vs `⌘`).

---

## What is intentionally out of FLOW (for now)

Image import/export, PDF/PSD, text layers, eyedropper, region select, vectorize,
generate-texture, BiRefNet core remove-bg — see `AGENTS.md` deferred list.
Add a FLOW section when a feature ships, not before.
