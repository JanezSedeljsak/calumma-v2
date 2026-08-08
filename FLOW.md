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
- **Tools island** (top to bottom): a 3-column tool grid (Pen, Eraser, Shape, Select, Fill);
  a contextual options section below it that changes with the selected tool (shape/selection
  sub-picker + fill toggle for shape tools, brush size for the tools that use one — not Fill
  or the selection tools); a quick-colour section (two clickable swatches to flip between two
  colours, a larger swatch that opens the full macOS colour panel, and a drag-to-scrub hue
  strip); the AI menu pinned at the bottom.
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
Swift. Pan is clamped with slack rather than pinned: the paper can be dragged around at any
zoom, including a fitted one, as long as half of it (`limits::PAN_KEEP_VISIBLE`) stays on
screen. `Fit` still centres the paper.

Space-pan is a hold, not a mode — it ends on key-up wherever focus is, and on app
deactivation, so a Space held across ⌘-Tab cannot leave the board stuck panning.

---

## Projects and persistence

| Concern | Behaviour |
| --- | --- |
| Store | OS-native app-data dir + `Calumma/calumma.sqlite` (macOS: `~/Library/Application Support/…`) |
| Autosave / explicit save | Engine dirty flag + `⌘S`; tab switch and close save first |
| One board per project | Bounded document size chosen at create time |
| Export image | **Shipped** — PNG / JPEG / WebP / AVIF / PSD via File → Export. PDF is still deferred; PSD is layered (real per-layer opacity/blend mode/pixels, hand-written encoder since ImageIO can only read PSD). |
| Import image / PSD | **Shipped** — new project from PNG / JPG / AVIF / WEBP / PSD / HEIC / SVG (SVG rasterized on import) |
| Clipboard paste artwork | **Shipped** — `⌘V`, drag-and-drop, or click on the island (creates a new project) |
| Import into an existing board | **Shipped** — drag-and-drop onto the canvas island, or `⌘V`, both add the image as a new layer (see Selection below) |

Composite flatten (`Document::composite_rgba`) and single-layer extraction (`Document::layer_rgba`)
live in `engine/core`; the actual PNG/JPEG/WebP/AVIF **encode** happens in the shell via
`ImageIO`/`CGImageDestination` (`ImageEncode.swift`), mirroring how decode already works.

---

## Layers and ops

- Raster layers (sparse 256×256 tiles); optional non-destructive mask.
- Add layer: `⌘⇧N` (shell).
- Clear active layer: `⌘⌫` — clears just the active **selection** instead if one exists.
- Each layer row keeps only the visibility toggle and a delete button directly visible; every
  other layer action lives behind a single `…` icon (`AppIcon.more`) that opens a per-layer
  settings popover (`LayerSettingsCard.swift`): **Copy** (PNG, or SVG if the layer is vector
  content), **Duplicate** (cheap, shares tile data via `Arc` until edited), **Merge Down**
  (composites onto the layer below respecting opacity/blend mode/adjustments, then removes
  the source — disabled when the layer below is Paper), **Reset Transform**, an **Opacity**
  slider, a **Blend Mode** picker (Normal / Multiply / Screen — see `AGENTS.md` → Layers for
  why only these three), and five **Filter** sliders (brightness, contrast, vibrance,
  saturation, gamma — levels black/white points were removed as redundant with
  brightness/contrast) with a reset button. All of it is live on the canvas
  and non-destructive — nothing here is undo-tracked (matches add/remove layer), and there is
  no explicit "bake into pixels" action; `merge_layer_down` and PSD export are the only two
  places any of it gets baked into concrete bytes today.
- Canvas resize: width/height fields docked at the bottom of the layers panel, commit on
  Enter (`Document::resize`). Top-left anchored; shrinking never discards off-canvas tile
  data, so growing back restores it exactly.
- **Transform (`⌘T`):** shows scale/rotate handles around the *active* layer (selected via
  the layers panel — clicking a layer's content directly on the canvas doesn't pick it yet,
  see `plans/02-layer-click-to-select.md`). Drag a corner to scale — proportional by
  default, hold **Shift** for free (non-uniform) scale; drag the handle above top-center to
  rotate; drag inside the box to move. Fully live and non-destructive on the canvas; a
  "Reset Transform" action in the layer's `…` popover clears it back to identity.
- **Remove Background:** AI menu on the tools island → macOS Vision via `calm_engine_run_op` when available.
  Shell never mutates the stack after the op. Details: `AGENTS.md` → AI ops.

## Selection

Three selection tools share one grid slot on the tools island (`M` cycles to whichever was
used last), with the specific shape chosen from the options panel below the tool grid:
rectangle, ellipse, and freehand lasso. A selection is a shape (not a persisted
document-sized mask) — `engine/core/src/selection.rs`'s `Selection`/`SelectionShape` store
just the rect/ellipse endpoints or the lasso polygon, and coverage is computed on demand by
reusing the same coverage math the Rect/Ellipse paint shapes already use. The outline
renders by reusing the existing shape-preview and stroke-preview GPU pipelines rather than a
dedicated marching-ants pass — a known simplification: the outline briefly stops rendering
while a *different* tool's live paint preview is on-screen at the same time, reappearing
once that drag ends.

| Shortcut | Action |
| --- | --- |
| `M` | Selection tool (rect / ellipse / lasso — remembers the last one used) |
| `Esc` | Deselect |
| `⌘C` | Copy — the selection (from the active layer) if one exists, otherwise the whole
  composited canvas. Always PNG on the clipboard. |
| `⌘X` | Cut — copies the selection, then clears those pixels from the active layer. No-op
  without a selection. |
| `⌘⌫` | Clears the selection's pixels if one exists, otherwise clears the whole active
  layer (existing binding, now selection-aware) |
| `⌘V` | Paste — clipboard image becomes a new layer, positioned at the selection's origin
  if one exists, otherwise at (0, 0) |

Selection-scoped copy/cut only ever reads/writes the **active layer**, matching Photoshop's
default `⌘C` (not "Copy Merged"). Paste always adds a new layer at the top of the stack
("forward") rather than inserting directly above whatever was active — a deliberate
simplification, not a real "insert above" primitive.

## Export

File → Export → PNG / JPEG / WebP / AVIF / PSD (moved out of the toolbar into the native
menu bar, alongside Settings under the app menu — `CalummaApp.swift`'s `.commands`, not a
toolbar button anymore). The raster formats flatten the full layer stack
(`Document::composite_rgba`, respecting
visibility, masks, opacity, blend mode, and adjustments) and opens a native save panel. PSD
is layered rather than flattened — each raster layer becomes a real PSD layer with its own
opacity and blend-mode signature (`engine/io/src/psd.rs`, hand-written since ImageIO can only
*read* PSD, not write it; RAW/uncompressed channel data, not PackBits RLE). PDF export is not
implemented.

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
| `M` | Selection (rect / ellipse / lasso — last one used) | Yes (Ps Marquee) |
| `G` | Fill (bucket) | Yes (Ps Paint Bucket, shared with Gradient) |
| `⌘T` | Transform the active layer (scale/rotate/move) | Yes (Ps Free Transform) |
| `F` | Toggle shape fill | — |
| `[` / `]` | Brush smaller / larger | Yes |

### Layers / view

| Shortcut | Action |
| --- | --- |
| `⌘⇧N` | Add layer |
| `⌘⌫` | Clear the selection's pixels, or the whole active layer if no selection |
| `⌘C` / `⌘X` / `⌘V` | Copy / cut / paste — see Selection in the section above |
| `Esc` | Deselect |
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

PDF export (PNG/JPEG/WebP/AVIF/PSD export shipped instead — see Export above), layered PSD
**import** (we import the flattened composite only; PSD *export* is layered and shipped),
click-to-pick a layer on the canvas (per-layer transform itself is shipped — see
Layers and ops above — picking the transform target is still layers-panel-only),
text layers, eyedropper, vectorize, generate-texture, BiRefNet core remove-bg — see
`AGENTS.md` deferred list. Add a FLOW section when a feature ships, not before.
