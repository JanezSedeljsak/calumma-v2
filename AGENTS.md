# AGENTS.md

Calumma is a personal whiteboard: bounded project canvases you draw on with a pen or
shapes. Projects are grouped into **workspaces**; titlebar tabs switch workspaces, and each
switch clean-loads the workspace’s active project from SQLite.

**Ambition:** product depth and scale in the neighbourhood of GIMP, Photoshop, Krita, and
Figma — multi-layer documents, large canvases, dense interaction. Performance and
scalability are first-class constraints on every change, not afterthoughts. The chrome
stays clean and minimalistic (`docs/STYLE.md`); complexity lives in the engine, not in
cluttered UI.

**Read this file first, then `docs/FLOW.md` and `docs/STYLE.md`.** Follow the one rule
below before inventing architecture. Prefer extending what exists over adding parallel
systems.

---

## The one rule

**The engine owns all state and all compute. The shell owns nothing but UI knobs.**

Shell knobs only: active tool, active brush, color, brush size, ink opacity, flood tolerance,
blur strength, eyedropper sample size, shape fill, shape stroke and its color, panel visibility,
open tab ids, theme, **language**. Last-used shape and selection tools, tool taxonomy (`is_shape`,
brush-size, vector-mode visibility), hex RGB, copy/cut bytes, and workspace switch live in
the engine. Coordinate math, clamping, pixels, camera, history, ops dispatch,
and board visuals live in Rust/WGSL.

If you are about to do pan/zoom arithmetic, tile math, or layer-stack mutation in Swift —
stop. Call an FFI method instead and keep the logic in `engine/`.

This extends to product rules, not just math. Zoom steps, the log zoom curve, fit padding,
the project-color palette and which color a new project gets, import limits, lossy export
quality — all core constants and core functions, reached over FFI. Swift renders what the
engine reports (`CalmState.zoom_unit`, `CalmState.accent`, `CalmState.last_shape_tool`,
`CalmState.is_fit`, `calm_palette_color`) and never recomputes it — the zoom pill's Fit
button lights up on `is_fit` rather than on Swift comparing a zoom against a fit zoom.
Theme **values** are the exception that proves the rule: they come from `design/tokens.json`
and are pushed *into* the engine (`calm_engine_set_board_colors`) so no color is ever
hardcoded in Rust or WGSL.

---

## STRICT SCOPE LIMITATIONS (Do Not Build)

Calumma is a lightning-fast, **flat-hierarchy** engine aimed at 120 Hz. These limits are
load-bearing, not backlog: they keep every tile 256 KiB, every vector layer one GPU draw,
and selection a layer index. Do not add the missing half. A plan that needs one of these
is the wrong plan. The renderer work that *followed* from them — the LIFO atlas, unpadded
tile uploads, one vector draw per layer, and the `LayerData` table that replaced per-layer
bind groups — is shipped; see `docs/ENGINE.md` § Bind groups.

- **Pure RGBA8 only.** Tiles are 256×256×4 bytes, straight 8-bit RGBA — `TILE_BYTES` is
  262144 (256 KiB) and stays that. No CMYK, no 16-bit, no 32-bit HDR, no ICC / display-P3 /
  color-profile conversion in the engine. Color in is color stored. Convert at the
  shell boundary on import if an OS decoder hands you something else, then forget it.
- **Flat stack only.** The stack is a `Vec<Layer>`. No groups, no folders, no clipping
  trees, no non-destructive **adjustment layers** (a layer that reads the backdrop).
  Per-layer opacity, blend mode, mask, transform, and the existing `Layer.adjustments`
  slot are the filter model; they do not grow a graph. The rule behind all of it:
  **a layer's rendering never depends on another layer's contents.** Features that
  look like they need it get a *destructive* form instead — a clipping mask merges the
  two layers the moment it is applied, the way merge-down already bakes. Acting on
  several layers at once (shift-select rows, then align, distribute or drag) is fine
  and is not an exception: that is one gesture applied N times, not a relationship.
- **1:1 vector limit.** `LayerContent::Vector(VectorItem)` — exactly one item, never
  `Vec`. A second shape or stroke is a new layer. Clicking a vector selects the layer;
  there is no `(layer, item)` address. **Multi-select of vector items is permanently
  cancelled** (see `docs/plans/24-layer-multi-select.md`, which replaces it at the *layer*
  level). Do not build it.
- **Basic vector editing only.** Vectors are drawn, moved, and scaled. No node / point
  editing, no bezier handles, no per-item rotation on the GPU, no boolean ops.

---

## Repository map

| Path | Role |
| --- | --- |
| `engine/core` | Document, sparse tiles, camera, viewport culling, history, shapes, palette, `LayerContent` — no GPU |
| `engine/text` | System fonts, shaping, layout, caret/hit-test, glyph rasterizing (`cosmic-text`). Leaf crate; `core` depends on it |
| `engine/render` | wgpu; surface created by the shell; applies layer masks at upload |
| `engine/io` | SQLite projects + encode/decode |
| `engine/ops` | `Op` / `OpRegistry` dispatch; apply results into the document |
| `engine/ffi` | C ABI; **only** crate Swift links; platform op vtable |
| `platform/macos` | SwiftUI landing, tabs, editor chrome, Metal canvas, Vision ops, i18n loader |
| `translations/` | Locale JSON (`en.json` today). Not code — edit strings here |
| `design/` | Visual tokens only (`tokens.json`), SVG icons, `icon.png` (app icon master, `./manage.py icon`) |
| `docs/` | All prose docs: `FLOW.md` (product flow), `STYLE.md` (design system), `ENGINE.md`, `RENDERING.md`, plus the gitignored `todo.md` + `plans/`. Only `README.md`, `AGENTS.md`, `CLAUDE.md` stay at the root |
| `cli/` | Python helpers + leaf tools (`_helpers.py`, tokens, purity, …). Deps in `requirements.txt` |
| `manage.py` | Task runner (Python 3.14). Prefer this over Make. |

Dependency direction:

```
text  ← std + cosmic-text (leaf)
core  ← std + small utils + text
render / io / ops  ← core
ffi  ← core, render, io, ops
Swift shell  ← ffi only (via Calumma.h)
```

`calumma-core` must stay free of wgpu / objc / metal / windows. Enforce with
`./manage.py purity`.

---

## How to work in this repo

1. **Change engine behaviour in Rust.** Add or extend `#[no_mangle]` FFI in `engine/ffi`,
   update `platform/macos/Calumma/Bridge/Calumma.h` and the Swift `Engine` wrapper in the
   same change. They are not cross-checked automatically.
2. **Change visuals in WGSL** (`engine/render/src/shaders/board.wgsl`) and mirror any SDF
   or tool discriminant in Rust (`engine/core/src/shape.rs`). Build validates shaders via
   naga in `build.rs`.
3. **Change chrome in Swift** using shared components in `UI/Components.swift` and tokens
   from `Tokens.generated.swift`. Do not sprinkle one-off fonts/colors/padding.
4. **After `design/tokens.json` edits:** `./manage.py tokens`.
5. **After Rust engine edits that affect the app:** `./manage.py test` (and rebuild ffi /
   open Xcode via `./manage.py dev` when touching the shell).
6. **No comments** in `.rs`, `.swift`, `.wgsl`. Name things clearly instead.
7. **Do not edit generated** `Tokens.generated.swift` by hand.
8. **Keep files small and single-topic** (below).

---

## File size and layout

Files stay small and each one answers a single question. A file that has grown past
**~400 lines**, or that has picked up a second concern, gets split — do it as part of the
change that would have grown it, not as a later cleanup.

The test is topical, not numeric: if you can name two things a file does, that is two files.

| Split when | Example |
| --- | --- |
| Two concerns share a file | `camera.rs` (zoom / pan / fit) vs `viewport.rs` (culling, device size, projection) |
| A type's helpers outgrow it | `palette.rs` holds project colors + `BoardColors`, not `document.rs` |
| A view file mixes screens | `ProjectSettingsCard.swift`, `PasteArtworkIsland.swift`, `WindowChrome.swift` split out of the screens that use them |

Rust: prefer a new module in the same crate over a new crate; `impl` blocks may live in a
different module than the `struct` (that is how `viewport.rs` extends `Camera`). Swift: one
screen or one reusable component per file; shared primitives stay in `UI/Components.swift`.

Do not split a file just to hit a number — a cohesive 450-line file beats three files that
have to be read together.

---

## Projects and navigation

- DB path: OS-native app-data directory + `Calumma/calumma.sqlite`, resolved by
  `ProjectStore::default_path` (`engine/io/src/store.rs`) via the `dirs` crate —
  never a hardcoded Unix path. macOS: `~/Library/Application Support/Calumma/…`;
  Linux: `~/.local/share/Calumma/…`; Windows: `%APPDATA%\Calumma\…`.
- Landing: name + resolution, presets from tokens, recents list, Paste Artwork island.
  Same view (`NewProjectView`) serves the separate, smaller **New Project** window opened
  by the editor `+` / `⌘N`; it reflows to one column below `Tokens.Window.wideLayoutWidth`.
- Artwork import: drop / `⌘V` / click on the Paste Artwork island creates a project sized to
  the image with the pixels in the first paint layer. Decode is ImageIO in the shell; the
  engine takes **premultiplied** RGBA over FFI and unpremultiplies in Rust
  (`calm_project_create_from_image`). Cap is `limits::IMPORT_MAX_SIDE`.
- Every project carries an **accent color** (`Document.accent`, `projects.accent` in SQLite).
  Core picks one from `palette::PROJECT_COLORS` at create time; the shell shows it on
  landing recents and project thumbs. Workspaces also carry an accent for their titlebar
  chip. Rename / recolor projects via `calm_project_rename` / `calm_project_set_accent`;
  workspaces via `calm_workspace_rename` / `calm_workspace_set_accent`. The palette itself
  is document data served from core (`calm_palette_color`), not a theme token.
- Editor: **titlebar workspace tabs** (right of traffic lights). Switch = save/close current
  → open the workspace’s active project (full reload). `+` adds a project to the active
  workspace; **extend** opens the workspace/project overlay with cached thumbnails.
- One board per project; bounded paper (not infinite canvas). The two ends of the zoom range
  are set independently and share no constant: the floor fills ~20% of the viewport
  (`MIN_ZOOM_FILL`), and the ceiling is whatever puts `MIN_VISIBLE_DOC_SIDE` (16) doc pixels
  across the short viewport side, under a flat `MAX_ZOOM_HARD` of 64×. Past
  `CRISP_PIXEL_ZOOM` the board magnifies tiles nearest-neighbour so deep zoom shows pixels
  rather than a blur of them.

---

## Layers

```rust
pub enum LayerContent {
    Raster(TileGrid),           // sparse 256×256 RGBA tiles
    Vector(VectorItem),         // exactly one parametric shape or freehand path
    Text { run, tiles },        // editable run + a tile cache rebuilt from it
}
```

- **Text** (`engine/text`, `core/src/text_layer.rs`, `core/src/text_edit.rs`) keeps its
  pixels in a `TileGrid` like any painted layer, but that grid is a *cache* of `run` —
  `text_layer::resync` clears and re-rasterizes it on every change. That is the whole trick:
  because `tiles()` returns `Some` for text, compositing, masks, opacity, blend modes,
  adjustments, thumbnails, GPU upload and PNG/PSD/SVG export need no text-awareness at all,
  while the run stays editable forever. Two consequences to respect when touching layer
  code: branch on `layer.tiles().is_none()` rather than `!is_raster()` when you mean "has no
  pixels" (`is_raster()` is **false** for text), and never write text tiles to SQLite — the
  run is the source of truth (`layers.text_data`, `content_kind = 2`).
  The same cache is why **paint tools refuse a text layer** (`Document::tool_block`, below):
  anything a brush, shape, fill or clear committed there would be wiped by the next `resync`.
  `Document::rasterize_text_layer` drops the run and keeps the tiles, and `merge_layer_down`
  calls it on the destination first. Structural edits (remove/duplicate/merge/resize,
  switching layers, undo/redo) `commit_text()` first —
  a session indexes a layer by position, so it must not outlive a stack that moved.
- Glyph work lives in **`engine/text`**, a leaf crate over `cosmic-text` (system font
  discovery via `fontdb`, shaping, layout, caret and hit-testing, rasterizing to RGBA).
  `calumma-core` depends on it. Font enumeration is engine-side on purpose — the shell must
  never ask AppKit for a font list it might not be able to draw. `fonts.rs` resolves the
  installed families **once** into a sorted, case-folded registry that also records which
  bold/italic cuts each family really ships (`calm_font_family_styles`), so `family_exists`
  is a binary search and `set_text_family` can refuse a name nothing can shape.
- Caret questions are answered against the **shaped layout**, never against the string:
  a wrapped paragraph is one `BufferLine` laid out as several rows, so `layout.rs` picks the
  row by glyph byte range (`run_span`) rather than by `line_i`, and horizontal steps go
  through cosmic-text `Motion` so one press crosses a whole grapheme cluster.
- A typing session (`Document::text_edit`) is **one** undo step: tiles *and* the run are
  snapshotted when it opens, and a single `TileDiff` + `RunDiff` lands when it closes
  (`History::push_layer_text`). The run has to be in the step because it is what the project
  stores — restoring only pixels would let the undone text come back on the next open.
  Per-keystroke history would flood the budget for no benefit.

- `layer.transform: Option<LayerTransform>` (`engine/core/src/transform.rs`)
  — offset/scale/rotation around the layer's `content_bounds()` center, on any layer that
  has content bounds — text included, since its tiles carry a transform row like any other
  and the run stays editable underneath (`⌘T` transform *mode*, not a toolbar tool). Same
  non-destructive contract as
  everything else on `Layer`: never baked into tile bytes. Live view applies
  it as a per-layer GPU uniform in `vs_tile` (`board.wgsl`); flattening
  (`composite_rgba`/`layer_rgba`/PSD + SVG export, all via
  `Document::copy_layer_into_rgba`) resamples it with nearest-neighbor,
  not bilinear — a known, disclosed gap between the (smooth, GPU-linear-
  filtered) live view and a flattened/exported result at extreme
  scale/rotation. The flatten walk is clipped to the transformed content
  AABB (`LayerTransform::transformed_aabb`), the same span vector flatten
  already used. Inside transform mode, clicking a layer's painted pixels
  retargets the transform to it (`Document::layer_at` — pixel-accurate,
  mask/transform/opacity-aware, skips Paper) and starts a move drag in the
  same gesture; clicking the active layer's own pixels always keeps it, so an
  overlapping layer above cannot steal the target. Clicking empty space (or
  `Esc`) exits; the Select tools remain for region marquee/lasso.
- **Move** (`Tool::Move = 15`, tools island) is translation without `⌘T`: click
  a vector item or a layer's painted pixels (`Document::begin_move_at`) and
  drag. Empty space and Paper are no-ops. Arrow keys call
  `nudge_move_target` — selected vector item first, otherwise the active
  layer's `transform.offset` when Move or transform mode is on. Transform is
  a *toggle on Move* (options panel / `⌘T`): on, the same grab shows
  scale/rotate handles and selects the layer; off, it only drags. `V` stays
  vector mode.
- **Paper** (`Layer::paper`) is an ordinary raster layer, name-matched via
  `Layer::is_paper()`, pre-filled fully opaque white at creation — not a
  cheap vector fill. It is paintable/eraseable/editable like any other
  layer; clearing it exposes transparency through to the desk. It is *not*
  a full-size white bitmap any more: `TileGrid::fill_uniform` gives every
  whole tile the same copy-on-write allocation, so Paper costs one tile
  until it is painted on (see Residency below). `merge_layer_down` refuses to
  merge anything into Paper.
- Optional `layer.mask: Option<Vec<u8>>` (full-document coverage 0–255). Masks do **not**
  mutate tile bytes; the renderer multiplies alpha when uploading GPU tiles.
- `layer.opacity: f32` (0–1, default 1) and `layer.blend_mode: BlendMode`
  (Normal/Multiply/Screen) — same non-destructive contract as masks: never
  baked into tile bytes, applied at GPU-upload time (opacity, via the same
  CPU step masks use) or via per-layer GPU pipeline selection (blend mode,
  since it needs the destination framebuffer — see `board.wgsl`'s `fs_tile`
  premultiply + the three `tile_pipeline_*` blend states in `renderer.rs`).
- `layer.adjustments: Option<Adjustments>` (`engine/core/src/filters.rs`) —
  brightness/contrast/vibrance/saturation/levels, `None` = neutral. Also
  applied at CPU upload time, no shader involvement (unlike blend mode,
  adjustments only ever read the source layer's own pixels). One entry point:
  the sliders in `LayerSettingsCard` (`calm_engine_set_layer_adjustments`).
  They ride `CalmDeferredSlider`, which keeps the knob local and hands the
  engine only the value still standing after 100ms of quiet — a rebake per
  emitted value is what a CPU-baked adjustment costs, and a drag emits a lot
  of them. The engine-side `nudge_layer_adjustment`
  (`limits::ADJUSTMENT_NUDGE_STEP` / `GAMMA_NUDGE_STEP`,
  `calm_engine_nudge_layer_adjustment`) is still there and still tested, but
  the menu-bar Filters menu that was its only caller is **gone** — a menu of
  Increase/Decrease pairs next to a panel of sliders was clutter, and the
  chrome stays minimal.
- **Which tools a layer accepts is one question with one answer**
  (`engine/core/src/tool_gate.rs`). `Document::tool_block(tool) -> ToolBlock` is the only
  rule set — the shell greys a button out on it, the engine refuses a press on it, and the
  brush ring stands down on it, so none of the three can drift. A text layer leaves Move,
  `⌘T` and the Text tool; a vector layer leaves those plus the pen and the shapes, because
  `vector_mode_locked` pins vector mode on and their result becomes a new layer under the 1:1
  limit; a locked layer leaves the eyedropper and Move, both of which pick their own target.
  `Document::rasterize_layer` (`rasterize.rs`) is the way out of the first two. Do not add a
  second predicate: `active_layer_accepts_paint` survives only for the commands that are not
  a tool press (paste, clear), because they have no tool to name and so nothing to explain.
- **Clipping is destructive, and that is the whole design**
  (`engine/core/src/merge.rs`). `clip_layer_down` is `merge_layer_down` with
  the source's alpha first multiplied by the base's raw tile alpha —
  `merge_down_inner` is literally one function with a `clip` flag. There is no
  `clipped` state, no clip group, no schema column, and the renderer never
  learns the word: after the action there is one ordinary layer, so export is
  free and there is no CPU/GPU rule pair to keep identical. The base's *raw*
  alpha is what clips, because the base keeps its own opacity/mask/adjustments
  afterwards and those then govern the merged result once — Photoshop's
  clipping-group semantics. It stands down on a base carrying a transform:
  the source bakes into document space while the base's tiles sit in its own,
  so the alpha would be misaligned by exactly that transform.
- `Document::duplicate_layer`/`merge_layer_down`/`clip_layer_down`/`resize`
  are **not** undo-tracked, matching the existing precedent that `add_layer`/
  `remove_layer` aren't either — structural layer-list/document edits sit
  outside the tile-diff history model on purpose, not as an oversight.
- **Vector layers** (`core/src/vector.rs`, `core/src/vector_edit.rs`,
  `core/src/vector_svg.rs`, `render/src/vector_draw.rs`) hold **exactly one**
  `VectorItem` — a parametric `Shape` or a freehand `VectorPath`. A second
  shape is a new layer (`Document::push_vector_item`). Two rules keep them
  honest: **parameters are the storage**, so moving or scaling edits the
  parameters and never resamples pixels; and the **board and the exporter
  evaluate the same distance functions** (`Shape::distance` in Rust,
  `shape_distance` in `board.wgsl`), so live view and flatten agree.
  - Live drawing is one GPU draw per vector layer: a `VectorShapeInstance`
    (`vs_vector_shape`) or a run of stroke segments, inserted into the
    per-frame draw list (`Renderer::build_layer_draws`) in stack order
    against tile layers. There is nothing to coalesce inside the layer.
  - Clicking a vector selects **the layer** (`Document::vector_item_at`).
    Move and `⌘T` then move / scale that layer's one item
    (`begin_vector_item_drag`). Picking treats a closed shape as solid
    (`VectorItem::pick_distance`) even when it is drawn as an outline.
  - A resize edits the *parameters* — `VectorItem::set_scaled` about the
    item's `geometry_bounds` centre, re-derived from the pointer-down
    capture every frame like `set_translated`. `ink_pad` (stroke half-width,
    plus an arrow's head) is subtracted from both the box and the pointer's
    reach, because a resize deliberately leaves ink weight alone.
  - Known gaps: a *rotated* vector layer draws its parametric shape
    unrotated live (the shader's SDFs are axis-aligned) while flatten/export
    stay correct; a filled closed freehand path has no GPU path at all and
    appears only once flattened; and item edits are not undo-tracked, the
    same as adding a layer or any other structural edit.
- `Document.selection: Option<Selection>` (`engine/core/src/selection.rs`) is a **document**-
  level concept, not a layer or a mask — a rect/ellipse/lasso shape (parameters only, not a
  persisted `width×height` buffer) that scopes copy/cut/clear to a region instead of a whole
  layer. Coverage is computed on demand (`SelectionShape::contains`), reusing the same
  math the Rect/Ellipse paint shapes already use. The commands that are not a pointer drag —
  `deselect` / `select_all` / `invert_selection` — live in `selection_edit.rs`, the way
  `text_edit.rs` and `vector_edit.rs` extend `Document` from their own modules. Invert has no
  buffer to flip for the parametric shapes, so it always produces the `SelectionShape::Mask`
  the wand already built, filled through `SelectionMask::from_predicate` (one rayon task per
  row, because unlike a flood it asks about every pixel of the canvas).
- **A shape carries a fill and a stroke independently** (`engine/core/src/shape.rs`).
  `Shape::region_distance` is the one SDF evaluation; `fill_distance` and `stroke_distance`
  are the two parts taken off it, either of which may be `None`. Painting a shape means
  blending both samples in order — `ink_sample` on the CPU, `shape_ink` in `board.wgsl`,
  which composite to the same result. colors never live on `Shape` (it also answers where a
  *selection* rectangle is): they come from `VectorShape`/`VectorPath`, or from
  `Document::shape_paint` for a raster commit.

---

## AI ops

**Shipped:** Remove Background on macOS via Vision only. Enum slots and registry mocks
exist for other kinds; do **not** implement them until explicitly requested.

```
Swift "Cut BG"  →  calm_engine_run_op(RemoveBackground, layer)
                 →  OpRegistry resolves Platform (Vision vtable)
                 →  VNGenerateForegroundInstanceMaskRequest
                 →  OpOutput::Mask
                 →  engine attaches mask + history step
```

- Shell never chooses Core vs Platform and never edits the layer stack after an op.
- Install platform ops once at engine startup (`VisionPlatformOps.install`).
- Platform wins when `available()` is true; otherwise the op is greyed out.
- Call ops only through `OpRegistry` / `calm_engine_run_op`, never ad hoc.
- Mask is non-destructive (tiles unchanged); renderer applies it at upload. Undo uses
  history `MaskDiff`.
- Prefer `run_op` off the main thread; `Inner` is mutex-protected. Platform `run` must
  not throw — Rust wraps the vtable in `catch_unwind`.

### Core vs platform

| **core** (default) | **platform** (justify) |
| --- | --- |
| Pure pixel/geometry compute | OS already ships a good model |
| Identical everywhere | Needs ANE / Vision / Core AI |
| Small enough to bundle | Large, licensed, or OS-managed |

Deferred (do not start): core BiRefNet / `ort`, Vectorize (`vtracer`), SuggestShape,
GenerateTexture, Image Playground / `ImageCreator`.

---

## Internationalisation (UI strings)

**User-facing text does not live in Swift or `tokens.json`.** It lives in
`translations/<lang>.json` (currently only `translations/en.json`).

| Piece | Responsibility |
| --- | --- |
| `translations/*.json` | Source of truth for copy (flat `key` → string) |
| `platform/macos/.../L10n/` | Load JSON from the app bundle (folder resource), expose `L10nCatalog` |
| `AppModel.language` | Runtime language knob (same idea as `theme`) |
| Settings sheet | Switch theme + language at runtime |

Rules for agents:

- Add or edit UI strings in `translations/en.json` (and future locale files). Never hardcode
  button/menu labels in `.swift` when a key exists.
- Access strings via `@Environment(\.l10n)` / `app.l10n` / `L10nStore.catalog` (bridge code).
- Loading + language switching is **platform** work. Keep `translations/` itself
  platform-agnostic JSON so a future Windows shell can reuse the same files.
- Only `en` is required today; `AppLanguage` is an enum you extend when adding locales.
- Preset labels in `design/tokens.json` are product data (resolutions), not i18n chrome.

### Dynamic strings (`{0}`, `{1}`, …)

When a translation needs runtime values, put numbered placeholders in the JSON — never
build sentences with `+` / string interpolation in Swift:

```json
"removeProjectNamed": "Remove project with name {0}.",
"layerNamed": "Layer {0}"
```

Platform code fills them in order via `L10nCatalog.format` / `formatKey` (simple
`{n}` → argument replace):

```swift
l10n.formatKey("removeProjectNamed", projectName)
l10n.formatKey("layerNamed", "\(index + 1)")
```

Placeholders are zero-based (`{0}`, `{1}`, …). Keep whole phrases in the locale file so
word order can change per language. Do not use Swift `String(format:)` / `%@` for UI copy.

## Styling

Visual tokens live in `design/tokens.json` → `./manage.py tokens` → `Tokens.*` (radius,
space, type, window, color, presets). Engine name constants live in `calumma_core::names`.
CLI paths/binaries live in `cli/constants.py`.

Compose `CalmText`, `CalmField`, `CalmRow`, `calmSurface()`, `CalmChip`, etc. Theme colors
via `@Environment(\.themeColors)`; copy via `@Environment(\.l10n)`.

1. Islands (`CalmIsland`) carry a thin `Tokens.Light/Dark.islandBorder` stroke. Text/number
   inputs, buttons, and list rows carry a stronger `controlBorder` (focused inputs:
   `controlFocusBorder`) via `calmSurface(bordered:focused:)`. Everywhere else — chips,
   swatches, the tool grid, sliders — separate surfaces by background contrast only.
2. Controls use `Tokens.Radius.sm` / `md`. Islands use `Tokens.Radius.island` (rounded) and
   sit apart with a minimal gap and window margin (`Tokens.Space.sm`), not flush.
3. Custom Canvas/`AppIcon` drawings only — no icon packs / SF Symbols as product icons.
4. Light and dark from tokens; push desk / grid / paper-border into the engine via
   `calm_engine_set_board_colors`. Never hardcode a color in `.rs` or `.wgsl`.
5. Filled controls; hover = luminance shift. One height for inputs and buttons alike:
   `Tokens.Control.height` (tools panel keeps its own denser 24pt scale).
6. Inline color picker (`QuickColorPicker`) — overlapping swatches, HSB field, hue, hex.
7. Nothing **drawn on the board** may be a SwiftUI view — paper, strokes, grid, and the
   layer hover outline are WGSL. Small chrome *controls* may float over the canvas island
   (the zoom pill sits bottom-trailing inside it); panels stay side-by-side islands.

Details: `docs/STYLE.md`.

---

## Performance and scalability

This is a drawing tool aimed at pro-app workloads (large boards, many layers, long
sessions). Prefer speed carefully — measure, then optimise. Not a secrets vault, but also
not a place to pile micro-opts that muddy the code for single-digit percent gains.

- Live strokes/shapes preview on the GPU; CPU commits on pointer-up into sparse tiles. A
  brush stroke previews through an offscreen coverage target (`render/src/stroke_coverage.rs`)
  so its own overlapping segments union rather than compound — the same maximum the CPU
  accumulates in `core/src/coverage.rs`, so the stroke does not change on pointer-up. The blur
  brush is the one exception to pointer-up commit: it has no color to preview, so it paints
  as the pointer moves.
- Dirty-flag rendering; idle board submits nothing.
- Tile pixels are `Arc` COW; history shares unchanged tiles (`Arc::make_mut` on write).
- Cap history by memory budget; design for documents that outgrow a single bitmap. Cold undo
  entries shrink rather than being dropped (`core/src/history_tile.rs`): a flat tile collapses
  to its four bytes, anything else is zstd. **The gate is `Arc::strong_count == 1`, not age** —
  a snapshot still shared with the live document costs nothing today, so compressing it would
  force the very copy the sharing avoids. The sweep runs on the autosave tick, never on the
  paint path, and is bounded per tick because it holds the engine lock.
- Painting APIs take **screen** coordinates; convert once in the engine.
- Engine `Inner` is behind a `Mutex` so ops can run off the main thread.
- Scalability checklist on structural changes: sparse tiles stay sparse, history does not
  deep-copy whole layers, GPU uploads stay dirty-region scoped, ops do not block the
  render loop longer than necessary.

### Residency: what is allowed to be in memory

**One document is resident at a time — the one you are working on.** Everything else lives
in SQLite. There is no document cache, no per-workspace pool, and nothing to evict on a
timer; switching workspaces or projects goes through `Inner::close_document`, which saves
the old document, drops it, and calls `Renderer::release_document` so the GPU textures and
the atlas slots keyed by its layer ids go with it. Add a new way to leave a
document and it must route through that same function — otherwise its tiles stay in VRAM,
where nothing will ever evict them (`sync_tiles` only runs with a document open).

Inside the resident document, memory is spent for speed on purpose and clawed back by
sharing rather than by unloading:

- Tiles are `Arc` copy-on-write. `TileGrid::fill_uniform` gives **one** allocation to every
  tile a solid fill covers whole, so Paper costs 256 KB instead of 256 KB × tile count; the
  first stroke on any of those tiles forks it and nothing else notices. `ProjectStore`
  re-establishes the same sharing on load (`tile::uniform_color`), so a reopened project is
  as cheap as a new one.
- History keeps its full budget for the open document (`HISTORY_MEMORY_BUDGET_BYTES`) —
  undo staying instant is the whole point — and dies with it.
- `calumma_core::memory::document_memory` is the measurement, exact rather than estimated:
  it counts each allocation once by address, so shared tiles are not double-counted, and
  `history_bytes` is what history holds *alone*. It is served over FFI as `CalmMemory`
  (`calm_engine_memory`) and shown in the Settings sheet. Reach for it before claiming a
  memory win.

### `unsafe` Rust — threshold rules

Do **not** overuse `unsafe`. Default to clean, safe Rust.

**Allowed**

- FFI / C ABI boundary (`engine/ffi`): null checks, `CStr`, `Box::from_raw`, wgpu
  `create_surface_unsafe`, platform vtable callbacks — always wrapped in `catch_unwind`,
  never unwind into Swift.
- A **proven** hot path where benchmarks (or clear asymptotic cost) show a **real** gain —
  keep the block tiny; invariants via naming + tests, not comments.
- Necessary raw pointer work the API forces (Metal layer handle, opaque engine ptr).

**Not allowed**

- `unsafe` for a tiny or speculative speedup (e.g. skipping a bounds check LLVM already
  elides).
- Large `unsafe` regions (“big unsafe”) to squeeze marginal cycles — prefer clean safe Rust.
- Copying unsafe patterns into `core` / `ops` / `render` “because FFI does it”.

Rules for agents:

1. Prefer safe code first. Only reach for `unsafe` when you can state the gain in concrete
   terms (throughput, latency, allocations avoided) and it is not a micro-optimisation.
2. If the safe alternative is clear and the unsafe block would be large or hard to audit,
   choose safe Rust even if it is slightly slower.
3. New `unsafe` in `calumma-core` needs a strong justification; pixel helpers that only
   save a checked index are below the bar unless measured on a real stroke/commit path.
4. FFI `unsafe` is expected at the boundary — keep it thin, centralise helpers
   (`with_inner`, string free), and do not leak raw pointers into higher crates.

---

## Canvas / render

Frame loop, dirty flags, and the pan/zoom performance strategy (GPU tile atlas, overview
LOD, motion mode) are documented in `docs/RENDERING.md`, not repeated here.

- Viewport-sized Metal surface; paper positioned by camera matrix in WGSL.
- Layer pixels, vectors, live previews, and handles are GPU-scissored to the paper
  (`Camera::paper_scissor`); the desk and paper border stay unclipped.
- Swift owns the `MTKView` / CAMetalLayer; Rust borrows the layer pointer (no retain).
- Layer hover = dashed outline in the shader, not a Swift overlay.
- Board **chrome** — guides, transform and vector-item frames, the text session's box and
  caret, the hover outline — is measured in *screen* pixels, not document units, so it is the
  same size at every zoom. Guides ride `vs_guide`/`fs_guide`, everything else
  `vs_overlay`/`fs_overlay`; both take document-space endpoints and read their width in screen
  px. Ink-shaped previews (a live stroke, a lasso, a selection's marching ants) stay on
  `vs_stroke`, where the brush is measured in document units because that is what it will
  commit as. Adding chrome means adding to the overlay pass, never to the stroke pass.

### WGSL naming

Never branch on bare literals (`tool == 1u`). Use named consts matching Rust
(`TOOL_LINE`, …) and `switch`. Keep discriminants aligned with `calumma_core::Tool`.

---

## Testing and tooling

```
python3 -m venv .venv && .venv/bin/pip install -r requirements.txt
./manage.py tokens # design/tokens.json → Swift Tokens
./manage.py icon # design/icon.png → AppIcon.appiconset (Pillow)
./manage.py test # cargo test --workspace
./manage.py coverage # llvm-cov + per-crate %% table in the log
./manage.py lint # clippy + ruff + purity
./manage.py check # fmt + lint + test
./manage.py purity # core has no platform/GPU deps
./manage.py dev # build ffi, xcodegen, open Xcode
./manage.py package # Release .app, ad-hoc signed, wrapped in dist/Calumma-<version>.dmg
```

`./manage.py` re-execs into `.venv` when that folder exists, so you do not need to activate it.

Rust unit tests live in `engine/<crate>/tests/<module>.rs`, one file per module under test
(`camera.rs` → `tests/camera.rs`), not in a `#[cfg(test)] mod tests` block inside the source
file — logic files stay logic, and `cargo test` already treats each `tests/*.rs` file as its
own integration-test crate against the library's public API, so this is free.

**One exception, `engine/render` only:** a crate-private module an integration test cannot
reach (`renderer`, `tile_atlas`, `overview`, `PanCache`'s `pub(crate)` half, `stroke_coverage`)
keeps its tests in a `#[cfg(test)] mod` at the bottom of its own file. Widening the API to
`pub` so a `tests/` file could reach it would be a worse trade than the one this rule is
protecting. Those tests share one headless device from `render/src/test_gpu.rs` — a
`wgpu::Device` with no surface, which is enough to build an atlas, a pan cache or an overview
pass, and they return early instead of failing where no adapter exists. The `Renderer` itself
still needs a real `wgpu::Surface`, so what its own tests cover is what it *decides* — blend
state per blend mode, bind-group and vertex layouts, visible/retained tile spans — not what it
draws.

Distribution: `.github/workflows/main.yml`'s `macos-dmg` job builds and publishes. On a
`push` to `main` it only runs if `version-check` (diffs `engine/Cargo.toml`'s version
against the previous commit) says the version changed; it always runs on a `v*` tag push
or a manual `workflow_dispatch`, and — since it packages a release — it also requires
`core-linux`, `core-windows` (skipped-is-OK, dispatch-only), and `macos` to have passed,
so a red lint/test job in the same run can never ship a release anyway. It runs
`./manage.py package` and publishes the `.dmg` + `.sha256` as **GitHub Release** assets
(GitHub Packages hosts only npm/Maven/NuGet/RubyGems/container registries — a `.dmg`
cannot live there). Version comes from the tag, falling back to `[workspace.package]
version` in `engine/Cargo.toml`; it is stamped into the bundle as `MARKETING_VERSION`.
Builds are **ad-hoc signed, not notarized** — there is no Developer ID in CI secrets, so
Gatekeeper blocks the first launch until the user right-clicks → Open.

App version stays in sync with zero manual Swift-side edits, in two layers:

1. The **committed** `project.pbxproj`: a local pre-commit hook (`xcodegen-version` in
   `.pre-commit-config.yaml`, triggered on `engine/Cargo.toml`) runs `./manage.py xcodegen`
   whenever that file changes, so a version bump never leaves a stale `MARKETING_VERSION`
   baked into the tracked project file waiting for someone to happen to build the app —
   same shape as the pre-existing `app-icon` hook regenerating `AppIcon.appiconset` from
   `design/icon.png`.
2. The **built app**, regardless of route (`manage.py dev`/`build`, a raw `xcodebuild`, or
   Xcode.app's own Run): the "Stamp version from Cargo.toml" build phase
   (`platform/macos/project.yml`, `postCompileScripts`) overwrites the built `Info.plist`'s
   `CFBundleShortVersionString`/`CFBundleVersion` with `./manage.py version` on every
   build, so even a still-stale `pbxproj` (a fresh clone that hasn't run pre-commit yet)
   can't ship the wrong version. It runs after Compile Sources but before Code Sign, so
   the signature is never invalidated. `./manage.py package`'s tag-resolved version has to
   win over that self-heal, so it passes `CALUMMA_VERSION_OVERRIDE` as an env var, which
   the script checks first (`cli/package_macos.py`).

Expectations:

- High coverage on `engine/core` (camera, tiles, history, shapes, paint commit).
- `engine/ops` registry tests: platform beats core, `available()` gating, failed ops leave
  the document untouched.
- Pre-commit: fmt, clippy, swift-format, no-comments, purity.

`cli/_helpers.py` holds shared paths, cargo helpers, and design-token accessors. Leaf tools
under `cli/` import from it. `manage.py` is the CLI entrypoint. Python deps are pinned in
`requirements.txt` (Pillow for the app icon, ruff for format/lint) — no extra CLI binaries
like oxipng.

Cargo workspace root is `engine/Cargo.toml` (rustfmt/clippy live there). Swift format
config is `platform/macos/.swift-format` only.

Pin versions in `[workspace.dependencies]`. Never `*` or bare `^`.

---

## Deliberately deferred

Vector *rotation* on the GPU (see Layers; per-item undo is planned with document
history, `docs/plans/01-document-undo.md`), BiRefNet / `ort`,
GenerateTexture model manager, SuggestShape,
Vectorize (`vtracer`), font embedding in PDF export (the exporter is shipped and layered, but
text rides as pixels), layered PSD import (import is flattened composite only;
PSD, SVG *and PDF export* are layered and shipped — see `docs/FLOW.md`), picking a layer by clicking it outside
transform mode as a *modifier* (the Move tool on the tools island is the path; Option-click and ⌘-click are
both already Pan), text *selection*
(the Text tool ships with a caret only — no shift-arrow, no styled ranges) — add
only as considered features, not by restoring old app code.

**Shipped from this list:** Select All / Invert Selection (`⌘A` / `⌘⇧I`), shape fill *and*
stroke together, workspaces (titlebar tabs + extend overlay), Eyedropper
(`I` / tools island; samples the composited pixel under the cursor into the active ink
swatch), vector layers (`V` / tool options; one item per layer, moved and scaled with
`⌘T` and the Move tool), text layers (`T` / tools island),
Move tool (tools island; pick-and-drag, Transform toggle / `⌘T` for scale/rotate). See `docs/FLOW.md`.

**Now carrying plans** in `docs/todo.md`: undo for the rest of the document (`01`),
GPU adjustment evaluation (`23`, which grows the shipped `LayerData`
table with the LUT and opacity). Vector multi-select (`10`) is closed by the 1:1 rule — do
not build it.
