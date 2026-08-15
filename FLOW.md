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
   └──── close last workspace tab ───┘  │
                    │                   │
                    │        New Project window (⌘N / titlebar +)
                    │
                    └── Settings (sheet)
```

| Screen | Purpose |
| --- | --- |
| **Landing** | Create a project (name + resolution / presets), open a recent, or import artwork |
| **Editor** | Draw on one board; titlebar **workspace** tabs; floating tool + layers islands |
| **New Project** | Separate small window with the same create form; opens over the Editor |
| **Settings** | Theme + language (shell knobs only) |

Landing create/open wraps the project in a **workspace** (or reopens the workspace that
already contains it). Closing the last workspace tab returns to Landing.

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
from it adds the project to the **active workspace** and closes the window. `⌘N` is disabled
while Landing is showing — that screen already *is* the create form.

---

## Editor layout

```
┌─ ● ● ●  ●[workspace] ●[workspace] [+] [extend] ─── ⚙ ───┐
├────┬────────────────────────────────────────┬────────────┤
│ 🛠 │          canvas island (Metal)          │   layers   │
│ AI │                       [− ▭ + 100% Fit] │            │
└────┴────────────────────────────────────────┴────────────┘
```

- **Workspace tabs:** one shared titlebar capsule (tabs + `+` + extend); switch = save →
  open that workspace’s active project. Accent dot opens rename / recolour for the
  **workspace**. Open tab order is persisted across launches.
- **+:** new project into the active workspace.
- **Extend:** scrollable overlay of **open** workspace tabs with cached project thumbnails
  (cropped to painted pixel bounds); click a project to open it inside that workspace.
  Create / delete workspaces live here.
- **Tools / layers / canvas:** three rounded, bordered islands, full-height, separated by a
  minimal gap and window margin (`space.sm`) — each has its own `islandBorder` stroke.
- **Tools island** (top to bottom): a 2-column tool grid (Move, Pen, Eraser, Shape, Select, Fill,
  Eyedropper, Text); a contextual options section below it that changes with the selected
  tool (shape/selection sub-picker + fill toggle for shape tools, font / size / alignment
  for Text, brush size for the tools that use one — not Fill, Eyedropper, Text, Move, or the
  selection tools — and ink opacity for Pen, shapes, and Fill; Eraser stays a full erase);
  a colour section (two equal quick swatches, a saturation/brightness field, a hue strip, and
  a hex field); the AI menu pinned at the bottom.
- **Board:** Metal surface clipped as its own island. Desk fill, grid, and the paper border
  come from tokens via `calm_engine_set_board_colors` — in light mode the desk matches the
  island surface; in dark mode the desk is a step darker than the window background so the
  board field sits recessed against the raised side islands. The paper border inverts with the theme
  (dark ring on the light board, light ring on the dark board). Layer pixels, vectors,
  previews, and handles are scissored to the paper — content may sit off the board, but
  only the overlap with the whiteboard is drawn.
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

### Projects and workspaces: colour and name

New projects get a random colour from the core palette (`palette::PROJECT_COLORS`), stored
on the project row. It appears as the recents thumbnail tint (and as the artwork preview when
a cached thumb exists). Workspaces carry their own accent on the titlebar chip; the chip’s
dot opens a card with the workspace name and palette. Open workspace tabs persist across
launches.

---

## Drawing and canvas

| Action | How |
| --- | --- |
| Paint / place shape | Click-drag on the board (pointer down → move → up). Engine converts **screen** coords. |
| Move a layer or vector item | Select **Move** on the tools island, then drag painted pixels or a vector item. Arrow keys nudge the same target. `⌘T` is still scale/rotate. |
| Constrain a shape | Hold **Shift** while dragging **Rect** or **Ellipse** (and their marquee twins) for a square or circle. Corner-anchored, and the *longer* side wins, so the shape fills the drag. Press or release Shift mid-drag and the board snaps immediately — the clamp is derived from the raw drag on every frame, not baked in on the last mouse-move. Line, Arrow, Triangle and Pentagon are unconstrained (angle snap and regular-polygon lock are different clamps, not built). |
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

Scroll-wheel and trackpad panning pass `scrollingDelta` through **unnegated** — AppKit has
already applied the system's "natural scrolling" preference, so any sign flip in the shell
would fight the user's setting rather than honour it. Scroll pan also carries a zoom-
dependent gain (`Camera::scroll_pan_gain`, `limits::SCROLL_PAN_MAX_GAIN`): a notch is a
fixed pixel amount, so without it a zoomed-out board crawls. Gain is 1 at Fit and never
drops below 1, so zooming in never makes scrolling slower. Pointer **drag** panning has no
gain — it tracks the cursor one-for-one by definition.

---

## Projects and persistence

| Concern | Behaviour |
| --- | --- |
| Store | OS-native app-data dir + `Calumma/calumma.sqlite` (macOS: `~/Library/Application Support/…`) |
| Autosave / explicit save | Engine dirty flag + `⌘S`; tab switch and close save first |
| One board per project | Bounded document size chosen at create time |
| Export image | **Shipped** — PNG / JPEG / WebP / AVIF / HEIC / PSD / SVG via File → Export, plus per-layer **Export…** in the layer card. PDF is still deferred; PSD and SVG are layered (PSD: real per-layer opacity/blend mode/pixels, hand-written encoder since ImageIO can only read PSD. SVG: vector layers stay geometry, painted layers embed a cropped PNG). |
| Import image / PSD | **Shipped** — new project from PNG / JPG / AVIF / WEBP / PSD / HEIC / SVG (SVG rasterized on import) |
| Clipboard paste artwork | **Shipped** — `⌘V`, drag-and-drop, or click on the island (creates a new project) |
| Import into an existing board | **Shipped** — drag-and-drop onto the canvas island, or `⌘V`, both add the image as a new layer (see Selection below) |

Composite flatten (`Document::composite_rgba`) and single-layer extraction (`Document::layer_rgba`)
live in `engine/core`; the actual PNG/JPEG/WebP/AVIF **encode** happens in the shell via
`ImageIO`/`CGImageDestination` (`ImageEncode.swift`), mirroring how decode already works.

---

## Text

- **Text tool** (`T`, tools island). Click the board: a new **text layer** opens with the
  caret where you clicked, and glyphs land on the board as you type — no dialog, no commit
  step. Click an existing text layer with the tool, or double-click it in the layers panel
  (also on its context menu), to re-enter and retype it.
- The session ends when you click elsewhere, press `Esc`, pick another tool, switch layers,
  undo, or change the layer stack. A text layer created and left empty removes itself;
  emptying one that already existed is an ordinary edit. Anything typed becomes **one** undo
  step, and undoing it takes back the *text*, not only the pixels.
- Options while the tool is active: **font** (a searchable list of every installed system
  family, each row previewed in its own face), **size**, **line height**, **bold**,
  **italic**, **alignment**. Bold and italic are offered only for families that really ship
  that cut — the engine reports which faces it loaded. Changing the ink colour recolours the
  run you are typing. The style you last used carries to the next text layer.
- All keyboard input goes through `NSTextInputClient`, so dead keys, the accent popover, the
  emoji picker and IME compositions all work; a composition in progress is drawn at the
  caret. While typing, only ⌘-chords still act as editor shortcuts.
- A text layer is a normal layer everywhere else: opacity, blend mode, masks, filters, Remove
  Background, thumbnails, PNG/PSD/SVG export. `⌘T` transform mode is the exception and refuses
  it — change the size instead. Projects store the *text*, not its pixels, and re-render it
  on open.
- **Paint tools are refused on a text layer**, because its pixels are a cache the next
  keystroke rebuilds — a stroke would vanish silently. **Rasterize Text** (layer `…` card)
  turns it into ordinary pixels, one way, and merging a layer down onto a text layer does the
  same to the destination first so the merged pixels survive.

## Layers and ops

- Raster layers (sparse 256×256 tiles); optional non-destructive mask.
- **Text layers** carry an editable run plus a tile cache rebuilt from it (see Text above).
- **Vector layers** carry a *list* of items — one per shape drawn or stroke pen-drawn with
  **vector mode** on (`V`, or the toggle under the tool options). Items land in the active
  layer when it is already a vector layer, otherwise a new one is created and becomes active,
  so a drawing accumulates in one layer instead of one layer per shape. The layer row shows
  how many items it holds. Nothing is rasterized: the board evaluates the same
  distance functions the exporter does, so a vector stays sharp at any zoom, exports as real
  SVG primitives (`<rect>`, `<ellipse>`, `<path>`, …) and is stored as parameters.
- **Moving one item:** inside `⌘T` transform mode, click an item to select it and drag it on
  its own; the arrow keys nudge it and `⌫` deletes it. The corner and rotate handles still
  scale and turn the *whole* layer, so both levels stay reachable in one mode. A click on an
  outlined shape counts anywhere inside it, not only on the outline. Item edits are not
  undo-tracked, matching the rest of the vector path (adding an item isn't either).
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
- **Transform (`⌘T`):** a transient *mode* on the active layer — not a tools-island
  button (Select tools stay for region marquee/lasso; transforming a selection region
  is separate). Shows scale/rotate handles around the *active* layer. Drag a corner to
  scale — proportional by default, hold **Shift** for free (non-uniform) scale, the same
  polarity Photoshop's Free Transform uses and deliberately the *opposite* of Shift while
  drawing a shape, where it constrains; drag the
  handle above top-center to rotate; drag inside the box to move. Click outside the
  handles, press `Esc`, or pick another tool to exit the mode. Fully live and
  non-destructive on the canvas; a "Reset Transform" action in the layer's `…` popover
  clears it back to identity.
- **Click-to-pick a layer**, inside transform mode: clicking a layer's *painted pixels*
  on the board makes it the transform target, so you can walk a stack without going back
  to the layers panel, and the same press starts a move drag. Picking is pixel-accurate
  (`Document::layer_at` — respects the layer's transform, mask, opacity and visibility)
  and skips Paper, the same way merge-down does. Resolution order on a press is:
  corner/rotate handle → **another layer's pixels** → move-inside-the-box → exit. The
  layer-stack step sits above "move" on purpose: the transform box is `content_bounds()`,
  which is tile-granular (256×256) and for a small scribble can be the whole document, so
  taking Move on every click inside it would make picking unreachable. Clicking the
  active layer's *own* pixels always keeps it, so an overlapping layer above can never
  steal the target mid-transform. Picking only happens in transform mode — Option-click
  and ⌘-click are both already Pan (see Pointer modifiers), so there is no free modifier
  for a universal "pick under cursor" gesture.
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

File → Export → PNG / JPEG / WebP / AVIF / HEIC / PSD / SVG (moved out of the toolbar into the
native menu bar, alongside Settings under the app menu — `CalummaApp.swift`'s `.commands`, not
a toolbar button anymore). The raster formats flatten the full layer stack
(`Document::composite_rgba`, respecting
visibility, masks, opacity, blend mode, and adjustments) and opens a native save panel.

**PSD and SVG are layered** rather than flattened. Each raster layer becomes a real PSD layer
with its own opacity and blend-mode signature (`engine/io/src/psd.rs`, hand-written since
ImageIO can only *read* PSD, not write it; RAW/uncompressed channel data, not PackBits RLE); a
vector layer reaches the PSD rasterized, because this writer emits raster channels only and
losing the artwork would be worse. SVG (`engine/io/src/svg.rs`) keeps a vector layer as real
`<rect>` / `<ellipse>` / `<path>` geometry and gives every other layer an embedded PNG
`<image>`, cropped to its ink; a layer painted in a single colour (Paper, a flood fill) becomes
a `<rect>` instead, so a flat page costs bytes rather than megabytes of base64. Layer opacity
and blend mode ride along as `opacity` / `mix-blend-mode`; masks and adjustments are baked into
the pixels, as everywhere else. Text exports as pixels, not `<text>` — the font it needs is not
in the file. PDF export is not implemented.

**One layer at a time**: the layer `…` card has **Export…** next to Copy, writing just that
layer through the same encoders — SVG first for a vector layer (its geometry, via
`Document::layer_svg`), otherwise the raster format the save panel picks. Copy (`⌘C`-style, to
the clipboard) does the same split: SVG for vector layers, PNG for everything else.

## Menu bar

`CalummaApp.swift`'s `.commands` owns the whole menu bar. Beyond File → Export and the app
menu's Settings above:

- **Board** — Fit to View (`0`), Toggle Layers (`⌥⌘L`), Enter Full Screen (`⌃⌘F`).
- **Filters** — Increase / Decrease per filter (brightness, contrast, vibrance,
  saturation, gamma) plus Reset. A menu is discrete and an adjustment is continuous, so
  the menu is a **nudge** surface, not a second slider panel: each item steps the active
  layer by one `limits::ADJUSTMENT_NUDGE_STEP` (gamma: `GAMMA_NUDGE_STEP`) through
  `calm_engine_nudge_layer_adjustment`. The step and the clamp live in the engine — the
  shell does no arithmetic, it only names the item. `LayerSettingsCard` stays the one
  place filter *UI* lives, so nothing is duplicated. Acts on the active layer only,
  matching that card, and is disabled on Landing and when the active layer is Paper.
  Like the sliders, nudges are **not** undo-tracked (same precedent as add/remove layer,
  duplicate, merge and resize) — worth revisiting for all of them together rather than
  making the menu path alone undoable.
- **View is removed.** AppKit synthesises it for every app and SwiftUI cannot declare it
  away, so `MenuBarPruner` (`UI/MenuBarChrome.swift`) deletes it from `NSApp.mainMenu`
  after launch — matching on the selectors its items send, not their titles, which the
  system localises. Removing View also removes Enter Full Screen, hence its re-homing
  into Board above. Removing View does **not** affect project tabs: those are custom
  SwiftUI chips in the titlebar, unrelated to AppKit's native window tabbing (which is
  off anyway via `NSWindow.tabbingMode = .disallowed`).

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
| `3` | Triangle (side count; `T` moved to Text) | — |
| `5` | Pentagon (side count; was `Y`) | — |
| `T` | Text — click the board to type inline | Yes |
| `E` | Eraser | Yes |
| `M` | Selection (rect / ellipse / lasso — last one used) | Yes (Ps Marquee) |
| `G` | Fill (bucket) | Yes (Ps Paint Bucket, shared with Gradient) |
| `I` | Eyedropper (live sample under the cursor into the active primary/secondary swatch; loupe shows colour + hex) | Yes |
| Move tool | Tools island — click a layer's pixels or a vector item to drag it; empty space is a no-op. `⌘T` stays for scale/rotate. `V` stays vector mode. | Ps `V` is Move; that key is already vector mode here |
| `⌘T` | Transform mode on the active layer (scale/rotate/move); click another layer's pixels to retarget, click empty space or `Esc` to exit | Yes (Ps Free Transform) |
| `⌥⌘B` `⌥⌘C` `⌥⌘V` `⌥⌘S` `⌥⌘G` | Increase brightness / contrast / vibrance / saturation / gamma on the active layer by one `limits::ADJUSTMENT_NUDGE_STEP` | — (Ps has no per-filter chord) |
| `⇧⌥⌘` + the same letter | Decrease the same filter by one step | — |
| `⌃⌘F` | Enter / exit full screen (re-homed from the removed View menu) | macOS standard |
| `F` | Toggle shape fill | — |
| `⇧` (held while dragging) | Constrain Rect / Ellipse to a square / circle | Yes (Ps shape constrain) |
| `V` | Toggle vector mode (shapes and the pen commit as editable vector items) | — (Ps has no equivalent; closest is Figma's vector tools) |
| `←` `→` `↑` `↓` | Nudge the selected vector item, or the active layer when Move / `⌘T` is the current tool | Yes (Ps nudge) |
| `⌫` / `⌦` | Delete the selected vector item (falls back to the old clear behaviour when none is selected) | Yes |
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
| Scroll | Pan — follows the system scroll direction, and speeds up as the board zooms out |
| ⌘ + scroll or Option + scroll | Zoom toward cursor |
| Pinch | Zoom |

Cursors: crosshair on the board (including Eyedropper), **open hand** while space is held
or the middle button is armed, **closed hand** while actually panning, zoom-in while
⌘/Option held over the board, pointing hand on chrome controls.

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

PDF export (PNG/JPEG/WebP/AVIF/HEIC/PSD/SVG export shipped instead — see Export above), layered PSD
**import** (we import the flattened composite only; PSD *export* is layered and shipped),
picking a layer by clicking it *outside* transform mode as a *modifier* (the Move tool on the tools island is the path — click painted pixels or a vector item to drag; Option-click and ⌘-click stay Pan),
text *selection* (the Text tool ships with a caret only — no shift-arrow, no styled ranges),
vectorize, generate-texture, BiRefNet core remove-bg — see
`AGENTS.md` deferred list. Add a FLOW section when a feature ships, not before.
