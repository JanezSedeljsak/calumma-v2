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
Landing ──create / open recent / drop artwork──► Editor
   ▲                                 │  ▲
   └──────── close last tab ─────────┘  │
                    │                   │
                    │        New Project window (⌘N / titlebar +)
                    │
                    └── Settings (sheet)
```

| Screen | Purpose |
| --- | --- |
| **Landing** | Create a project (name + resolution / presets), open a recent, or import artwork |
| **Editor** | Draw on one board; titlebar project tabs; floating tool + layers islands |
| **New Project** | Separate small window with the same create form; opens over the Editor |
| **Settings** | Theme + language (shell knobs only) |

There is no separate “project browser” beyond Landing recents. Closing the last editor tab
returns to Landing.

**Landing and New Project are one view** (`NewProjectView`) in one landscape layout — you
get the same screen whether no project is open yet (Landing, in the main window) or one
already is (New Project, in its own smaller window). The Paste Artwork island scales with
the window rather than sitting at a fixed size. Window sizes come from
`design/tokens.json` → `Tokens.Window`.

---

## Landing

1. Enter a project name (default from i18n).
2. Set width × height, or pick a **preset** from tokens (product data, not i18n).
3. **Create** → engine creates SQLite project, opens it, fits camera, shows Editor.
4. **Recents** → open existing project (full load).
5. **Paste Artwork** island → import an image as a new project (below).
6. Settings gear → theme / language sheet.

### Paste Artwork

The island is a live import target, not decoration. Three ways in, all equivalent:

| Gesture | Behaviour |
| --- | --- |
| Click | Opens the system file picker filtered to supported formats |
| Drag and drop | Accepts a dropped file **or** raw image data from another app |
| `⌘V` | Pastes the clipboard image while that window is key (`paste:` on the responder chain, so ⌘V inside the name field still pastes text) |

Any of these creates a **new project sized to the image**, with the image blitted into the
first paint layer (`Layer 1`) above Paper, then opens it in the Editor.

Formats: **PNG, JPG/JPEG, AVIF, WEBP, PSD, HEIC/HEIF, SVG** (PSD imports the flattened
composite; SVG is rasterized on import, not kept as vector data). Clipboard paste
additionally accepts TIFF, because that is the form most macOS apps put images on the
pasteboard in. Decoding is ImageIO in the shell; anything larger than `IMPORT_MAX_SIDE`
(engine constant, 4096 px) is downscaled at decode time so a huge photo cannot blow up the
tile grid.

### New Project window

With a board already open, the titlebar **+** (or `⌘N`) opens a separate, smaller window
carrying the same landscape screen — form, presets, recents, Paste Artwork island. Creating
from it adds a tab to the Editor and closes the window. `⌘N` is disabled while Landing is
showing — that screen already *is* the create form.

---

## Editor layout

```
┌─ ● ● ●  ●[tab] ●[tab] [+] ─────────────────── ⚙ ─────────┐
├────┬────────────────────────────────────────┬────────────┤
│ 🛠 │          canvas island (Metal)          │   layers   │
│ AI │                       [− ▭ + 100% Fit] │            │
└────┴────────────────────────────────────────┴────────────┘
```

- **Tabs:** compact titlebar (right of traffic lights); switch = save → close → open selected.
  Each tab leads with its **project accent dot** — click it to rename or recolour.
- **Tools / layers / canvas:** three rounded, bordered islands, full-height, separated by a
  minimal gap and window margin (`space.sm`) — each has its own `islandBorder` stroke.
- **Tools island** (top to bottom): a 2-column tool grid (Pen, Eraser, Shape); a contextual
  options section below it that changes with the selected tool (shape sub-picker + fill
  toggle for shape tools, brush size for all of them); a quick-colour section (two clickable
  swatches to flip between two colours, a larger swatch that opens the full macOS colour
  panel, and a drag-to-scrub hue strip); the AI menu pinned at the bottom.
- **Board:** Metal surface clipped as its own island. Desk fill, grid, and the paper border
  come from tokens via `calm_engine_set_board_colors` — the desk is darker than the app
  background in both themes, and the paper border inverts with the theme (dark ring on the
  light board, light ring on the dark board).
- **Layers:** add / select / visibility / delete; first layer is **Paper**, a normal
  white-filled raster layer — paintable/eraseable like any other layer, not a background
  decoration. The list shows the topmost (frontmost) layer first, matching stack order.
  Hover shows a thumbnail popover; each row also carries a persistent thumbnail.
- **AI:** tools-island icon menu; Remove Background when Vision is available.
- **Zoom:** a pill pinned **bottom-trailing inside the canvas island** — `−`, slider, `+`,
  percentage, Fit. Range is canvas-relative: zoom out until the paper fills ~50% of the
  viewport, in up to 10× that floor (capped so ~400 doc px still span the short viewport
  side). The slider is logarithmic; the curve, the step factor, and the fit padding are all
  core (`Camera::zoom_unit` / `zoom_from_unit` / `step_zoom`, `limits::ZOOM_STEP`,
  `limits::FIT_PADDING`), so the shell only moves a 0…1 value.
- **Fit** fills the canvas island rather than leaving a wide margin — opening a project or
  pressing `0` puts the paper edge-to-edge in the viewport.

### Projects: colour and name

New projects get a random colour from the core palette (`palette::PROJECT_COLORS`), stored
on the project row. It appears on the titlebar tab and as the recents thumbnail tint. The
tab's dot opens a card with the project name and the palette; both persist immediately.

---

## Drawing and canvas

| Action | How |
| --- | --- |
| Paint / place shape | Click-drag on the board (pointer down → move → up). Engine converts **screen** coords. |
| Live preview | GPU stroke/shape while dragging; CPU commit into sparse tiles on pointer-up. |
| Pan | Scroll wheel / trackpad scroll; **middle-button drag**; Space-drag; or Option/⌘-drag |
| Zoom | Pinch; ⌘ + scroll; Option + scroll; or ⌘`=` / ⌘`-` |
| Fit to view | `0`, the zoom pill, or Board menu — fills the canvas island |
| Space-pan | Hold Space for temporary hand tool (Photoshop-style) |
| Maximise window | Double-click the titlebar (standard macOS zoom) |

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
| Import image / PSD | **Shipped** — new project from PNG / JPG / AVIF / WEBP / PSD |
| Clipboard paste artwork | **Shipped** — `⌘V`, drag-and-drop, or click on the island |
| Import into an existing board | **Not shipped** — import always creates a new project |

When export lands, document formats and menus here; keep encode/decode in `engine/io` and
file dialogs in the platform shell.

---

## Layers and ops

- Raster layers (sparse 256×256 tiles); optional non-destructive mask.
- Add layer: `⌘⇧N` (shell).
- Clear active layer: `⌘⌫`.
- **Remove Background:** AI menu on the tools island → macOS Vision via `calm_engine_run_op` when available.
  Shell never mutates the stack after the op. Details: `AGENTS.md` → AI ops.

---

## Shortcuts (macOS today)

Align new bindings with Photoshop where possible. Engine actions go through FFI; tool /
panel toggles are shell knobs.

### Global / menu

| Shortcut | Action | Photoshop-ish? |
| --- | --- | --- |
| `⌘N` | New Project window (disabled on Landing) | New document |
| `⌘V` | Paste clipboard artwork as a new project (Landing / New Project window) | Ps pastes into the document; we create one |
| `⌘S` | Save | Yes |
| `⌘Z` | Undo (Edit menu) | Yes |
| `⌘⇧Z` | Redo (Edit menu) | Yes (Ps redo varies by platform; we use ⌘⇧Z) |
| `⌘,` | Settings (theme / language) | macOS prefs |
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
| `E` | Eraser | Yes |
| `F` | Toggle shape fill | — |
| `[` / `]` | Brush smaller / larger | Yes |

### Layers / view

| Shortcut | Action |
| --- | --- |
| `⌘⇧N` | Add layer |
| `⌘⌫` | Clear active layer |
| `⌘=` / `⌘+` | Zoom in one core step (`limits::ZOOM_STEP`) |
| `⌘-` | Zoom out one core step |

### Pointer modifiers (board)

| Gesture | Action |
| --- | --- |
| Drag | Paint / shape |
| Space (hold) | Temporary pan (open-hand cursor, applied as soon as the key goes down) |
| Space-drag / Option-drag / ⌘-drag | Pan |
| **Middle-button drag** | Pan — same as space-drag, no key needed |
| Scroll | Pan |
| ⌘ + scroll or Option + scroll | Zoom toward cursor |
| Pinch | Zoom |

Cursors: crosshair on the board, **open hand** while space is held or the middle button is
armed, **closed hand** while actually panning, zoom-in while ⌘/Option held over the board,
pointing hand on chrome controls.

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

Image/PDF **export**, layered PSD (we import the flattened composite only), importing into
an existing board, text layers, eyedropper, region select, vectorize, generate-texture,
BiRefNet core remove-bg — see `AGENTS.md` deferred list.
Add a FLOW section when a feature ships, not before.
