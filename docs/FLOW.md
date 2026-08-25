# FLOW.md — how Calumma works

Base product documentation: screens, what you can do, navigation, canvas interaction,
shortcuts, persistence, and import/export. Companion to `AGENTS.md` (architecture) and
`docs/STYLE.md` (look). Platform shells implement chrome and key bindings; the engine
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

1. Enter a project name (default from i18n). **Return in the name field creates the
   project** — name plus the default size is the whole form for most projects, so the
   keyboard finishes it; an empty name still resolves to Untitled.
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
  open that workspace’s active project. Accent dot opens rename / recolor for the
  **workspace**. Open tab order is persisted across launches.
- **+:** new project into the active workspace.
- **Extend:** scrollable overlay of **open** workspace tabs with cached project thumbnails
  (cropped to painted pixel bounds); click a project to open it inside that workspace.
  Create / delete workspaces live here.
- **Tools / layers / canvas:** three rounded, bordered islands, full-height, separated by a
  minimal gap and window margin (`space.sm`) — each has its own `islandBorder` stroke.
- **Tools island** (top to bottom): a 2-column tool grid (Move, Select, Pen, Eraser, Blur,
  Shape, Fill, Eyedropper, Text). When **Move** is selected the options panel carries a
  **Transform** toggle — `⌘T` on the active layer, a *mode* not a tool, lit from engine
  state (`CalmState.transform_active`). Off, Move only drags a layer around; on, the same
  grab shows scale/rotate handles and clicking a layer's pixels selects it. Then a
  contextual options section below the grid that changes with the selected tool
  (shape/selection sub-picker + independent **Fill** and **Stroke** toggles for
  the shape tools that enclose an area — Rect, Ellipse, Triangle, Pentagon; Line and Arrow are
  outlines with nothing inside them and offer neither — a **brush**
  sub-picker for the Pen, font / size / alignment for Text, brush size for the tools that use
  one — not Fill, Eyedropper, Text, Move, or the selection tools — sample size for the
  Eyedropper (a circle under the cursor shows the area) — ink opacity for Pen, shapes,
  and Fill, strength for Blur, and tolerance for Fill and the magic wand; Eraser stays a full
  erase); a color section of **exactly three swatches — primary, secondary, tertiary — and
  never more**, plus a saturation/brightness field, a hue strip, and a hex field, all three
  editing whichever swatch is ringed; the AI menu pinned at the bottom.

  The three are the whole colour surface. There is no separate outline swatch, because an area
  shape already reads two of them **by role**: **primary is its outline, secondary is its
  fill** (`Document::shape_paint`), whichever swatch happens to be ringed for editing — so a
  rectangle comes out the same way every time instead of depending on what was clicked last.
  While a fill-capable shape tool is selected the first two tooltips say so. Line and Arrow
  have no interior and no second half: they are the ink, meaning the ringed swatch, as they
  always were.
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
  percentage, Fit. The two ends are independent: zoom out until the paper fills ~20% of the
  viewport, and in until ~16 doc px span the short viewport side (or 64×, whichever comes
  first) — far enough to work pixel by pixel, where the board switches to nearest-neighbour
  magnification so a pixel reads as a pixel. The slider is logarithmic; the curve, the step
  factor, and the fit padding are all core (`Camera::zoom_unit` / `zoom_from_unit` /
  `step_zoom`, `limits::ZOOM_STEP`, `limits::FIT_PADDING`), so the shell only moves a 0…1
  value.
- **Fit** fills the canvas island rather than leaving a wide margin — opening a project or
  pressing `0` puts the paper edge-to-edge in the viewport.

### Projects and workspaces: color and name

New projects get a random color from the core palette (`palette::PROJECT_COLORS`), stored
on the project row. It appears as the recents thumbnail tint (and as the artwork preview when
a cached thumb exists). Workspaces carry their own accent on the titlebar chip; the chip’s
dot opens a card with the workspace name and palette. Open workspace tabs persist across
launches.

---

## Drawing and canvas

| Action | How |
| --- | --- |
| Paint / place shape | Click-drag on the board (pointer down → move → up). Engine converts **screen** coords. |
| See the brush | Pen, Eraser and Blur draw a **ring at the pointer, the size of the brush** — document geometry, so it scales with the zoom exactly as the stamp does, with the line held at one screen pixel by `vs_overlay`. Two rings a pixel apart, light inside dark, because one colour cannot stay legible over both white paper and black ink. Under ~3px across it collapses to a dot. It is withheld exactly where a stroke would be refused — a text, vector or locked layer, or inside `⌘T` — so no ring means no stroke. `Document::brush_ring` owns every one of those rules; the shell only forwards the pointer (`calm_engine_set_pointer_hover`) and takes it away while panning or zoom-chording. |
| Move a layer or vector item | Select **Move** on the tools island, then drag painted pixels or a vector item. Arrow keys nudge the same target. Turn **Transform** on (options toggle or `⌘T`) for scale/rotate of that layer; the same press selects it. |
| Resize a vector item | Select it (Move or `⌘T`), then drag a corner of its box. Proportional by default, **Shift** frees the two axes — the same polarity as a `⌘T` corner. |
| Constrain a shape | Hold **Shift** while dragging **Rect** or **Ellipse** (and their marquee twins) for a square or circle. Corner-anchored, and the *longer* side wins, so the shape fills the drag. Press or release Shift mid-drag and the board snaps immediately — the clamp is derived from the raw drag on every frame, not baked in on the last mouse-move. Line, Arrow, Triangle and Pentagon are unconstrained (angle snap and regular-polygon lock are different clamps, not built). |
| Live preview | GPU stroke/shape while dragging; CPU commit into sparse tiles on pointer-up. A shape previews its fill *and* its border in their own colors, because `board.wgsl`'s `shape_ink` composites the same two parts, in the same order, that the commit does. |
| Pan | Scroll wheel / trackpad scroll; **middle-button drag**; Space-drag; or Option/⌘-drag |
| Zoom | Pinch; ⌘ + scroll; Option + scroll; or ⌘`=` / ⌘`-` |
| Fit to view | `0`, the zoom pill, or Board menu — fills the canvas island. The pill's Fit button reads as *on* (accent) while the board is already fitted and *off* (muted) once you pan or zoom away, the same way a selected tool reads. |
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
| Export image | **Shipped** — PNG / JPEG / WebP / AVIF / HEIC / PSD / SVG / PDF via File → Export, plus per-layer **Export…** in the layer card. PSD, SVG and PDF are layered (PSD: real per-layer opacity/blend mode/pixels, hand-written encoder since ImageIO can only read PSD. SVG: vector layers stay geometry, painted layers embed a cropped PNG. PDF: vector layers become real PDF paths, opacity and blend mode ride an `/ExtGState`, transparency an `/SMask`). |
| Import image / PSD | **Shipped** — new project from PNG / JPG / AVIF / WEBP / PSD / HEIC / SVG (SVG rasterized on import) |
| Clipboard paste artwork | **Shipped** — `⌘V`, drag-and-drop, or click on the island (creates a new project) |
| Import into an existing board | **Shipped** — drag-and-drop onto the canvas island, or `⌘V`, both add the image as a new layer (see Selection below) |

Composite flatten (`Document::composite_rgba`) and single-layer extraction (`Document::layer_rgba`)
live in `engine/core`; the actual PNG/JPEG/WebP/AVIF **encode** happens in the shell via
`ImageIO`/`CGImageDestination` (`ImageEncode.swift`), mirroring how decode already works.

---

## Text

- **Size sliders** (brush size, text size). Both run **1–1000 px** (text from 4), because a
  brush that covers a region and a headline set across a board are what those knobs are for.
  A range that wide cannot be linear on a 96pt-wide panel: the slider carries **0…1 of
  travel** and the engine places it on a **quadratic** curve (`size_curve::size_from_unit`,
  `limits::SIZE_CURVE_EXPONENT`), so half the track lands under a quarter of the range —
  where the sizes anyone dials by hand actually are — and the far half still reaches 1000.
  The shell never computes the curve, exactly as it never computes the log zoom curve.
  Beside every size slider is a **number field**: the exact way to 137px, which no
  panel-width slider can resolve. It clamps to the range on commit, not per keystroke, so
  typing on the way to a larger number is not clamped out from under you. `[` / `]` step the
  brush **proportionally** (`size_curve::step_brush_size`, 10% and never less than a pixel),
  so the whole range is reachable by key in under sixty presses. The slider always lands on
  whole pixels, so the number printed and the number typed are the same number.
  The **blur** brush is the one tool whose cost scales with the square of the size — it reads
  and rewrites its whole disc per pointer event, where the pen and eraser only commit at
  pointer-up — so the top of its range is deliberately expensive rather than free.
- **Brushes** (Pen sub-picker, Pen only). Four ways the pen lays ink down — **Pen**,
  **Marker**, **Crayon**, **Airbrush** — chosen from a 2×2 grid under the tool. They change
  the *character* of the ink, not how much of it there is: size and opacity stay their own
  sliders and keep working across all four. Each brush is an edge falloff, a flow, and an
  amount of paper grain (`engine/core/src/brush.rs`); Pen is hardness 1 / flow 1 / no grain,
  which is byte-for-byte the pen that existed before brushes did. Marker and Airbrush lay
  down less than full ink, so they read as translucent without touching the opacity slider;
  Crayon bites into its coverage with document-space value noise, so the tooth belongs to the
  paper and stays put under a stroke instead of sliding along with it. A soft brush's falloff
  reaches *inward* from the rim, so every brush covers exactly the width the size slider says.
  **A stroke is one wash, not a chain of dabs.** Coverage accumulates as a *maximum* over the
  stroke's segments and composites onto the layer exactly once, so passing over a pixel twice
  within one stroke cannot darken it. This is what makes low opacity usable at all: strokes
  used to blend each stamp onto the last, so a slow drag compounded its own overlaps into a
  dark beaded rope that got darker the slower you drew. Drawing over the same place with a
  *second* stroke still builds up — that is how you deepen a wash. The live preview does the
  same union on the GPU through an offscreen coverage target, so what you see while dragging
  is what lands on pointer-up.
  The brush is Pen-only, and vector mode hides the picker since a resolution-independent path
  has no raster coverage to shape.
- **Eraser hardness** (Eraser only). The eraser carries an edge but not a whole brush: grain
  and flow describe ink going down, and it is taking ink away, so it gets one **Hardness**
  slider instead of the picker. 100% is the complete, hard-edged erase Calumma has always had
  and stays the default; lower feathers the rim, thinning alpha and leaving the color it is
  eating away at intact rather than cutting a stamped-out hole. Coverage maxes within a
  stroke, so one soft pass leaves the rim standing and going over it again eats further in —
  the same build-up the brushes have, which is how a real soft eraser behaves.

- **Blur tool** (`U`, tools island). Softens the pixels already on the active layer instead
  of adding color to them — the first tool that reads the destination and writes a function
  of it. Size is the usual brush slider; **Strength** is how far each pixel travels toward
  its blurred neighbourhood (0 is a genuine no-op and leaves no undo entry). The kernel is
  three sliding box passes stacked as a Gaussian approximation, run over a snapshot of the
  region so the stamp cannot read pixels it has already written — reading its own output is
  what turns a blur into a directional smear. It works in premultiplied alpha, so softening
  the edge of a painted shape against transparency does not pull it toward black.
  Unlike every other stamp tool, blur commits **as the pointer moves** rather than at
  pointer-up: there is no color to preview on the GPU, so the board shows the real result
  and the tiles it touches accumulate into one snapshot — the whole stroke is still a single
  `⌘Z`. Dragging back over the same pixels on a later event blurs them further, the way a
  real brush builds up. It clips to the active selection (like the bucket) and refuses a text
  layer (like the pen), since `text_layer::resync` would wipe the stroke on the next
  keystroke.

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
  that cut — the engine reports which faces it loaded. Changing the ink color recolors the
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
- **Vector layers** carry **exactly one** item — one shape drawn or one stroke pen-drawn with
  **vector mode** on (`V`, or the toggle under the tool options). Each commit is a new layer;
  clicking a vector selects that layer. Nothing is rasterized: the board evaluates the same
  distance functions the exporter does, so a vector stays sharp at any zoom, exports as real
  SVG primitives (`<rect>`, `<ellipse>`, `<path>`, …) and is stored as parameters.
- **Fill and stroke together.** A shape carries an independent fill color and outline
  color, either of which may be off and both of which may be on — a white rectangle with a
  black 2px border is one shape, not two. `Shape` keeps the two as flags (`fill`, `stroke`)
  over one region SDF, so the interior and the annulus come off a single distance evaluation
  per pixel; the colors live on whatever is painting (`VectorShape.color` /
  `stroke_color`, or the document's **primary swatch for the outline and secondary for the
  fill** on a raster commit — see the tools island above). Stroke width shares the brush-size
  slider. Line and Arrow have no interior, so they ignore both
  flags and keep the one ink color they always had — resolved once in
  `Document::shape_paint`, so nothing downstream has to re-ask which tool it is dealing with.
  SVG export now writes real `fill` *and* `stroke`/`stroke-width` attributes rather than one
  or the other, which is a closer match to the format than the either/or it replaced.
  A shape saved before this carries its one color into whichever half it was being used as
  (`vector_blob` v3; v1 and v2 blobs still load).
- **Moving a vector:** with the Move tool or inside `⌘T`, click a vector to select its layer
  and drag it; the arrow keys nudge it and `⌫` deletes the layer. A click on an outlined shape
  counts anywhere inside it, not only on the outline. Item edits are not undo-tracked,
  matching the rest of the vector path (adding a vector layer isn't either).
- **Resizing a vector:** selecting it puts a box with four corner handles around it, and
  dragging one resizes *that item* — the layer's one shape. Proportional by
  default, **Shift** frees the two axes. It edits the shape's parameters, so nothing is
  resampled and the result is as sharp at any size as the original. Stroke weight stays put,
  the way it does in Figma and Photoshop.
  There is **no rotate handle on an item** — the board's shape SDFs are axis-aligned, so per-
  item rotation is deferred rather than half-offered. Rotating still means rotating the layer.
- **One frame at a time:** while an item is selected, the layer's own `⌘T` corners and rotate
  handle stand down, so the corners on screen always belong to the thing you last clicked.
  Clicking off the item drops the selection and hands the frame back to the layer.
- Add layer: `⌘⇧N` (shell).
- Clear active layer: `⌘⌫` — clears just the active **selection** instead if one exists.
- **A row is a thumbnail, a name, and one `…` button** (`AppIcon.more`) — nothing else. Every
  layer action lives in the popover it opens (`LayerSettingsCard.swift`), including the three
  that used to sit in the row: **Visibility** and **Lock** (toggles at the top of the card, so
  they show their state), and **Delete** (last, in `color.danger`, behind a divider so it is
  not adjacent to Duplicate). A hidden or locked layer still says so in its row, with a glyph
  rather than a button. The cost is that visibility is one click deeper than it was — the most
  frequent action in any layers panel — which is why it is the first thing in the card, under
  the pointer as the popover opens. The rest of the card: **Rename**, **Copy** (PNG, or SVG if
  the layer is vector content), **Duplicate** (cheap, shares tile data via `Arc` until edited),
  **Merge Down** (composites onto the layer below respecting mask/opacity/blend
  mode/adjustments, then removes the source — disabled when the layer below is Paper),
  **Clip to Below** (below), **Reset Transform**, an
  **Opacity** slider, a **Blend Mode** picker (Normal / Multiply / Screen — see `AGENTS.md` → Layers for
  why only these three), and five **Filter** sliders (brightness, contrast, vibrance,
  saturation, gamma — levels black/white points were removed as redundant with
  brightness/contrast) with a reset button. All of it is live on the canvas
  and non-destructive — nothing here is undo-tracked (matches add/remove layer), and there is
  no explicit "bake into pixels" action; `merge_layer_down`, `clip_layer_down` and PSD export
  are the only places any of it gets baked into concrete bytes today.
- **The filter sliders are deferred, the opacity slider is not.** An adjustment is baked on
  the CPU at upload time, so every value a drag emits costs a full-layer rebake;
  `CalmDeferredSlider` keeps the knob local and tells the engine only the value still
  standing after 100ms of quiet, which turns a drag into a handful of bakes. Opacity keeps
  its direct binding because its `%` readout reads the engine value and would stutter behind
  the knob.
- **Clip to Below** (`⌥⌘G`, or the card's button) bakes the layer through the alpha of the one
  underneath it and merges the two, right then. It is Photoshop's clipping mask as a
  **destructive action**, not a live compositing rule: there is no clipped flag, no clipping
  group, no schema column, and no layer whose rendering depends on another layer's contents
  (`AGENTS.md` → STRICT SCOPE). Clipping into a group of three means doing it twice — each
  apply leaves an ordinary layer that can be clipped into again. The base keeps its own
  opacity, mask and adjustments, which then govern the merged result once, so the clip reads
  the base's **raw** tile alpha rather than its composited alpha. Offered only where it can be
  honest: greyed out with no layer below, with Paper below, with a vector layer below, or with
  a base carrying a transform — the source bakes into document space while the base's tiles
  sit in its own, so the alpha would be off by exactly that transform. Not undo-tracked, same
  as Merge Down.
- **The list uses the height it has:** the stack takes every point the header above it and the
  Layer bounds fields below it do not, and scrolls once it runs out, rather than stopping at a
  fixed share of the island with dead space underneath. A floor keeps it from collapsing
  entirely in a short window. Renaming is still a double-click on the name, or the row's
  context menu, or Rename in the card.
- **Drag-reorder:** drag a row onto another row to put it there. Dropping *onto* a row rather
  than between rows is the whole contract — there is no insertion point to get off by one at
  either end. Move Up / Move Down stay in the `…` popover as the keyboard-reachable path. The
  panel draws the stack top-first while the document stores it bottom-first, so the shell
  hands `calm_engine_move_layer_row` the row it dragged and the row it dropped on and the
  engine owns the flip — Swift never computes a layer index. Order already persisted via
  `z_index`, so nothing new is saved.
- **Rename:** double-click a layer's name for an inline field, or Rename from its context
  menu. Double-click on a *text* layer still opens the text for editing (that came first), so
  the context menu is the path for those. `Enter` commits, `Esc` abandons. Names are trimmed
  and an all-whitespace name is refused. **Paper cannot be renamed, and nothing else can be
  renamed *to* Paper** — `Layer::is_paper` is name-matched, so merge-down, clip-down and
  click-to-pick all key off that string; letting it move would break all three silently.
- **Lock** (the padlock beside the eye): refuses everything that would change the layer's
  pixels or where they sit — paint, fill, clear, transform (`⌘T` and Reset Transform), Move,
  arrow-key nudge, the bounds strip, and vector item drags. A locked layer is also invisible
  to click-to-pick, so a lock cannot be stolen by clicking its pixels on the board. Locking
  the layer you are mid-transform on drops the handles rather than leaving a box that refuses
  to drag. Visibility, duplicate, export and the opacity/blend/filter sliders still work, and
  so does **delete** — a lock guards against the stray stroke, not against a deliberate press
  of the button next to it, which is how Photoshop and Figma behave too. Paper can be locked
  (it is the layer people paint on by accident most), it just cannot be renamed. Persists in a
  `locked` column on `layers`.
- Canvas resize: width/height fields docked at the bottom of the layers panel, commit on
  Enter (`Document::resize`). Top-left anchored; shrinking never discards off-canvas tile
  data, so growing back restores it exactly.
- **Layer bounds:** X / Y / W / H for the *active* layer (never the hovered one — hover
  already has the outline), in document pixels, in the same fixed strip above canvas resize.
  The numbers are `Document::layer_bounds`: tight to painted pixels (`opaque_bounds`), not
  the tile-granular `content_bounds()` the hover outline draws from, so the readout can read
  smaller than the box on the board by up to a tile. The transform is baked in, so it is a
  document-space AABB — a rotated layer's W/H grow as it rotates. Editable, commit on Enter:
  **position moves** (a `LayerTransform` offset, non-destructive, same as the Move tool) and
  **size only ever crops** — a number larger than the layer clamps rather than scaling the
  content up, since there are no pixels to invent; scaling up is `⌘T`. A layer carrying a
  scale or rotation moves but does not crop. The fields always mirror what the engine took,
  not what was typed. Values follow selection at pointer-up granularity, not live through a
  transform drag.
- **Transform (`⌘T`):** a transient *mode* on the active layer, reached from Move's options
  toggle or `⌘T` (which selects Move so the toggle is visible). Not a tools-island button
  of its own (Select tools stay for region marquee/lasso; transforming a selection region
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
  steal the target mid-transform. Move *without* transform already selects the layer it
  grabs (`begin_move_at`). Inside transform, picking is the retarget path above —
  Option-click and ⌘-click are both already Pan (see Pointer modifiers), so there is no
  free modifier for a universal "pick under cursor" gesture.
- **Remove Background:** AI menu on the tools island → macOS Vision via `calm_engine_run_op` when available.
  Shell never mutates the stack after the op. Details: `AGENTS.md` → AI ops.

## Selection

Four selection tools share one grid slot on the tools island (`M` cycles to whichever was
used last), with the specific one chosen from the options panel below the tool grid:
rectangle, ellipse, freehand lasso, and **magic wand**. Three of the four are shapes (not
persisted document-sized masks) — `engine/core/src/selection.rs`'s
`Selection`/`SelectionShape` store just the rect/ellipse endpoints or the lasso polygon, and
coverage is computed on demand by reusing the same coverage math the Rect/Ellipse paint
shapes already use. The outline renders by reusing the existing shape-preview and
stroke-preview GPU pipelines rather than a dedicated marching-ants pass — a known
simplification: the outline briefly stops rendering while a *different* tool's live paint
preview is on-screen at the same time, reappearing once that drag ends.

- **Magic wand** (`W`, in the select flyout). Click a pixel of the **active layer** and the
  contiguous region within **Tolerance** of its color becomes the selection. Reading the
  active layer rather than the composite is deliberate: clicking a sketch's background
  selects that layer's background, not the Paper showing through it. Alpha counts toward
  the tolerance, so the empty space around a drawing is selectable like any color.
  Tolerance is **one knob shared with the bucket**, because they are one traversal
  (`fill::flood_region`) — a wand that disagreed with the bucket about what "contiguous"
  means would be a bug report. Unlike the bucket, the wand ignores any existing selection
  when it floods: it *replaces* the selection, so letting the old one clip the new one would
  make a second click unable to grow past the first.
  This is the one selection whose answer is **stored** rather than derived —
  `SelectionShape::Mask`, one bit per pixel (8 MiB, not 64, for a full-canvas wand on an
  8K document), cropped to what the flood actually reached. Because it lives inside
  `SelectionShape`, paint clipping, copy, cut and delete needed no changes at all: they
  already go through `bounds()` and `contains()`. Its ants come from a boundary traced once
  at commit and merged into maximal runs, so the render pass never walks the bitmap however
  large the selection is. A click that reaches nothing leaves the selection untouched — an
  empty-but-present selection would silently clip every later stroke to nothing.
  Not in scope for this pass: global (non-contiguous) select-by-color, feathering,
  grow/shrink, and sample-all-layers.

| Shortcut | Action |
| --- | --- |
| `M` | Selection tool (rect / ellipse / lasso / magic wand — remembers the last one used) |
| `W` | Magic wand directly |
| `⌘A` | Select All — the whole canvas as one rect, matching Photoshop rather than the active
  layer's painted bounds |
| `⌘⇧I` | Invert Selection — everything the current selection leaves out, clipped to the
  canvas. With no selection it selects all, the way Photoshop answers an inverted nothing.
  The result is always a `SelectionShape::Mask`, since the parametric shapes have no buffer
  to flip; inverting a full-canvas selection reaches no pixel and so leaves *no* selection,
  the same rule the wand follows for a click that reaches nothing. |
| `Esc` | Deselect |
| `⌘C` | Copy — the selection (from the active layer) if one exists, otherwise the whole
  composited canvas. Always PNG on the clipboard. |
| `⌘X` | Cut — copies the selection, then clears those pixels from the active layer. No-op
  without a selection. |
| `⌘⌫` | Clears the selection's pixels if one exists, otherwise clears the whole active
  layer (existing binding, now selection-aware) |
| `⌘V` | Paste — clipboard image becomes a new layer, positioned at the selection's origin
  if one exists, otherwise at (0, 0). The board hands it straight over: **Move is selected and
  the new layer enters `⌘T`**, corners live, because placing it is the next thing anyone does.
  Dropping an image onto an open board is the same gesture by another route and behaves
  identically |

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
`<image>`, cropped to its ink; a layer painted in a single color (Paper, a flood fill) becomes
a `<rect>` instead, so a flat page costs bytes rather than megabytes of base64. Layer opacity
and blend mode ride along as `opacity` / `mix-blend-mode`; masks and adjustments are baked into
the pixels, as everywhere else. Text exports as pixels, not `<text>` — the font it needs is not
in the file.

**PDF** is the same layered walk written to a different format, and the one Calumma's layer
model maps onto most exactly: `layer.opacity` is `/ca` and `/CA`, the three blend modes are
`/BM` names that match one for one, a vector layer becomes real path operators (`re`, `c`,
`m`/`l`) rather than a picture of itself, and a painted layer becomes a cropped image XObject
with its alpha in a `/SMask` — PDF images carry no alpha channel of their own. One flip
matrix at the top of the page reconciles PDF's bottom-left origin with Calumma's top-left.
Masks and adjustments are still baked; text still exports as pixels, so a PDF's text is not
selectable yet. Page size is document pixels at 72 dpi by default (`calm_pdf_default_dpi`),
and the encoder is Rust (`engine/io/src/pdf.rs`) for the same reason PSD and SVG are:
`CGPDFContext` would need the shell to re-emit every shape.

**One layer at a time**: the layer `…` card has **Export…** next to Copy, writing just that
layer through the same encoders — SVG first for a vector layer (its geometry, via
`Document::layer_svg`), otherwise the raster format the save panel picks. Copy (`⌘C`-style, to
the clipboard) does the same split: SVG for vector layers, PNG for everything else.

## Menu bar

`CalummaApp.swift`'s `.commands` owns the whole menu bar. Beyond File → Export and the app
menu's Settings above:

- **Board** — Fit to View (`0`), Toggle Layers (`⌥⌘L`), Enter Full Screen (`⌃⌘F`).
- **Filters is removed.** It carried an Increase / Decrease pair per filter plus Reset,
  which is a discrete menu standing next to a panel of continuous sliders that already
  said the same thing — clutter, against the minimal-chrome rule, and the sliders were
  always the surface people used. The engine half it drove (`nudge_layer_adjustment`,
  `limits::ADJUSTMENT_NUDGE_STEP` / `GAMMA_NUDGE_STEP`, `calm_engine_nudge_layer_adjustment`)
  is still there and still tested; nothing in the shell calls it today. Its `⌥⌘G` is now
  Clip to Below.
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
| `0` | Fit to view — the zoom pill's Fit button carries the accent color while the board
  is already fitted (`CalmState.is_fit`, answered by `Camera::is_fit`) and the muted color
  otherwise, the same on/off reading a selected tool has | Ps `⌘0` is fit; bare `0` is our fit today |

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
| `W` | Magic wand (select by color) | Yes (Ps Magic Wand) |
| `U` | Blur | Ps puts Blur on `R`, which is Rectangle here, and `B` stays reserved for the brush family |
| `M` | Selection (rect / ellipse / lasso — last one used) | Yes (Ps Marquee) |
| `G` | Fill (bucket) | Yes (Ps Paint Bucket, shared with Gradient) |
| `I` | Eyedropper (live sample under the cursor into the active primary/secondary swatch; loupe shows color + hex; a circle shows the sample area) | Yes |
| Move tool | Tools island — click a layer's pixels or a vector item to drag it; Transform off, that is all it does. Transform on (options toggle or `⌘T`) adds scale/rotate handles and selecting a layer's pixels makes it active. Empty space is a no-op. `V` stays vector mode. | Ps `V` is Move; that key is already vector mode here |
| `⌘T` | Select Move and toggle transform mode on the active layer (scale/rotate/move); click another layer's pixels to retarget, click empty space or `Esc` to exit | Yes (Ps Free Transform) |
| `⌥⌘G` | Clip to Below on the active layer — see Layers | Yes (Ps Create Clipping Mask, though ours merges rather than clipping live) |
| `⌃⌘F` | Enter / exit full screen (re-homed from the removed View menu) | macOS standard |
| `F` | Toggle shape fill | — |
| `S` | Toggle shape stroke — independent of fill, so a shape can carry both | — (Figma has both by default) |
| `⇧` (held while dragging) | Constrain Rect / Ellipse to a square / circle; on a `⌘T` or vector-item corner, free the two axes instead | Yes (Ps shape constrain) |
| `V` | Toggle vector mode (shapes and the pen each commit as their own vector layer) | — (Ps has no equivalent; closest is Figma's vector tools) |
| `←` `→` `↑` `↓` | Nudge the selected vector item, or the active layer when Move / `⌘T` is the current tool | Yes (Ps nudge) |
| `⌫` / `⌦` | Delete the selected vector item (falls back to the old clear behaviour when none is selected) | Yes |
| `[` / `]` | Brush smaller / larger; with the Eyedropper, sample size | Yes |

### Layers / view

| Shortcut | Action |
| --- | --- |
| `⌘⇧N` | Add layer |
| `⌘⌫` | Clear the selection's pixels, or the whole active layer if no selection |
| `⌘C` / `⌘X` / `⌘V` | Copy / cut / paste — see Selection in the section above |
| `Esc` | Deselect |
| `⌘=` / `⌘+` | Zoom in one core step (`limits::ZOOM_STEP`) |
| `⌘-` | Zoom out one core step |
| `⌘A` / `⌘⇧I` | Select All / Invert Selection — see Selection above |

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

Layered PSD
**import** (we import the flattened composite only; PSD *export* is layered and shipped),
PDF text as *selectable* text (PDF export is shipped, but text rides as pixels — font
embedding needs `FontFile2`, `/Widths` and a `/ToUnicode` CMap),
picking a layer by clicking it *outside* transform mode as a *modifier* (the Move tool on the tools island is the path — click painted pixels or a vector item to drag; Option-click and ⌘-click stay Pan),
text *selection* (the Text tool ships with a caret only — no shift-arrow, no styled ranges),
vectorize, generate-texture, BiRefNet core remove-bg — see
`AGENTS.md` deferred list. Add a FLOW section when a feature ships, not before.
