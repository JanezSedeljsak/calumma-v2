# ENGINE.md — how Calumma's engine is built

The engine is everything Calumma knows how to *do*: the document, the pixels, the camera,
history, persistence, and the board on screen. The macOS shell owns a window, a Metal layer,
and a set of UI knobs; it owns no state and computes nothing. This file is the map of what
lives where and, more usefully, **why each boundary is where it is** — the reasoning that is
easy to lose once the code compiles.

Read this alongside:

| File | Answers |
| --- | --- |
| [`AGENTS.md`](../AGENTS.md) | The rules a change has to obey (start there). **STRICT SCOPE LIMITATIONS** is load-bearing for this crate. |
| [`docs/FLOW.md`](FLOW.md) | What the product does — screens, tools, shortcuts, I/O |
| [`docs/RENDERING.md`](RENDERING.md) | The **frame loop**, dirty flags, and the pan/zoom performance strategy in detail |
| this file | How the crates fit together, and how the renderer is built |

`RENDERING.md` and this file deliberately split: that one is *per-frame mechanics and
optimization history*, this one is *structure and rationale*. The renderer section below
covers what the renderer **is**; `RENDERING.md` covers what it **does each frame**.

---

## 1. The shape of the engine

Six crates, one workspace (`Cargo.toml`), strictly layered:

```
                    ┌──────────────┐
                    │ calumma-text │  cosmic-text: fonts, shaping, layout, spans, glyph raster
                    └──────┬───────┘  (leaf — knows nothing about documents)
                           │
                    ┌──────▼───────┐
                    │ calumma-core │  Document, tiles, camera, history, shapes, selection
                    └──┬────┬───┬──┘  (no GPU, no OS, no SQL)
             ┌─────────┘    │   └──────────┐
      ┌──────▼──────┐ ┌─────▼────┐ ┌───────▼──────┐
      │calumma-render│ │calumma-io│ │ calumma-ops  │
      │  wgpu / WGSL │ │  SQLite  │ │ Op registry  │
      └──────┬──────┘ └─────┬────┘ └───────┬──────┘
             └──────────┬───┴──────────────┘
                 ┌──────▼──────┐
                 │ calumma-ffi │  the only crate Swift links
                 └─────────────┘
```

**Why this split and not one crate.** Each edge is a constraint someone can violate by
accident, and the crate boundary is what makes that a compile error instead of a code review:

- **`core` must stay pure.** No wgpu, no objc, no Metal, no SQL. That is what lets every
  interesting piece of logic — coverage accumulation, flood fill, blur, SDF evaluation, tile
  math, history — be unit-tested without a GPU, a window server, or a temp database, and it
  is what makes a future non-macOS shell a shell problem rather than an engine rewrite.
  Enforced mechanically by `./manage.py purity`, which walks `core`'s dependency tree.
- **`text` is below `core`, not beside it.** Font enumeration, shaping and glyph rasterizing
  are a self-contained problem with one heavyweight dependency (`cosmic-text`). Keeping it a
  leaf means `core` can depend on it without `text` ever being able to depend back on a
  `Document`, and it keeps the font registry (which scans the system once at startup)
  reachable from the engine rather than from AppKit. **The shell must never ask the OS for a
  font list**, because it would be listing fonts the engine may not be able to shape.
- **`render`, `io` and `ops` are siblings.** None of them may see the others. The renderer
  cannot save; the store cannot draw; an AI op cannot do either. Everything they share, they
  share through `core`.
- **`ffi` is the only crate with a C ABI**, the only one Swift links, and the only place
  `unsafe` is routine. It is where the three siblings are finally allowed to meet.

`default-members = ["core", "ffi"]`, so a bare `cargo build` in `engine/` builds the two
crates that matter and pulls the rest in transitively.

### Build profiles

`[profile.dev]` and `[profile.test]` are `opt-level = 3`, not 0 — the engine is per-pixel
scalar loops end to end (tile compositing, mip downsampling, blur, flood fill), and at
`opt-level = 0` those run an order of magnitude slower, which makes every interactive
measurement taken against a dev build meaningless. Debug info is `line-tables-only` so
backtraces still work without paying for full DWARF on every link.

Release is `opt-level = 3, lto = "fat", codegen-units = 1, strip = "symbols"` — the pixel
helpers are small functions called from million-iteration loops, and cross-crate inlining
is the whole game. Release is deliberately **not** `panic = "abort"`: the FFI boundary
catches unwinds, so a Rust panic can never reach Swift, and aborting would turn a
recoverable engine error into an app crash.

---

## STRICT SCOPE LIMITATIONS

The engine is a 120 Hz rasterizer with a **flat** layer stack. These three rules are why
tiles stay 256 KiB, why a vector layer is one GPU draw, and why the atlas can stay a
LIFO free list with no padded uploads. Full wording:
[`AGENTS.md` STRICT SCOPE LIMITATIONS](../AGENTS.md). Do not add the missing half here.

- **Pure RGBA8 only.** `TILE_SIZE` is 256, `TILE_BYTES` is 262144. No CMYK, no 16/32-bit
  HDR, no ICC / color-profile conversion in the engine.
- **Flat stack only.** `Vec<Layer>`. No groups, folders, or adjustment *layers* (a layer
  that reads the backdrop). `Layer.adjustments` is a per-layer LUT, not a graph.
- **1:1 vector limit.** `LayerContent::Vector(VectorItem)` — exactly one item. Multi-select
  of vector items is permanently cancelled.

---

## 2. `core` — the document model

### The document

`Document` (`core/src/document.rs`) is the single source of truth for one open project:
layers, active layer, camera, history, selection, guides, and the tool knobs the shell
pushes in. **One document is resident at a time**; everything else lives in SQLite. There is
no cache and nothing to evict on a timer.

`document.rs` is large and is being split outward as it grows: `impl Document` blocks live in
`text_edit.rs`, `text_input.rs`, `text_select.rs`, `vector_edit.rs`, `move_edit.rs` and
`selection_edit.rs` — same type, one topic per file. `viewport.rs` extends `Camera` the same
way. The three text files are the split in miniature: the session's lifetime, the edits that
change the string, and where the caret and its anchor are.

### Layers

```rust
pub enum LayerContent {
    Raster(TileGrid),      // sparse 256×256 RGBA tiles
    Vector(VectorItem),    // exactly one parametric shape or freehand path
    Text { run, tiles },   // editable run + a tile cache rebuilt from it
}
```

**One vector item per layer is the whole selection model.** Clicking a shape selects that
layer; there is no `(layer, item)` address and nothing to iterate at draw time, so a
vector layer is one GPU draw call. A second shape is a second layer. See
`AGENTS.md` STRICT SCOPE.

**Text keeps its pixels in a `TileGrid` too, and that is the whole trick.** The grid is a
*cache* of `run`; `text_layer::resync` clears and re-rasterizes it on every change. Because
`tiles()` returns `Some` for text, compositing, masks, opacity, blend modes,
thumbnails, GPU upload and PNG/PSD/SVG export need no text-awareness at all — while the run
stays editable forever. Two consequences: branch on `layer.tiles().is_none()` when you mean
"has no pixels" (`is_raster()` is **false** for text), and never write text tiles to SQLite —
the run is what is stored. This holds for everything a run has since grown — a wrap width, a
selection range, style spans over byte ranges — because all of it is content the next `resync`
turns into the same flat RGBA the compositor already knew how to draw.

Everything else a layer carries is **non-destructive** and never baked into tile bytes:
`mask`, `opacity`, `blend_mode`, `transform`. They are applied either at GPU
upload (mask/opacity, on the CPU, per dirty tile) or by pipeline selection
(blend mode, which needs the destination framebuffer). The flatten path applies the same
things in the same order, which is why an export matches the board.

### Tiles

`TileGrid` (`core/src/tile.rs`) is a hash map from `TileCoord` to `Arc<Vec<u8>>` — 256×256
RGBA8, 256 KiB a tile (`TILE_BYTES` = 262144), straight (non-premultiplied) alpha. A row is
exactly 1024 bytes, which is why the renderer can upload the `Arc` without padding.
Three properties carry most of the engine's scalability:

- **Sparse.** An empty region costs nothing. A diagonal flick across an 8K board allocates
  the tiles along the ribbon, not the rectangle enclosing it.
- **Copy-on-write.** Tiles are `Arc`; a write does `Arc::make_mut`. History shares unchanged
  tiles with the live document rather than copying them, and `TileGrid::fill_uniform` gives
  every whole tile a solid fill covers **one** allocation — which is why Paper costs 256 KB
  instead of 256 KB × tile count until something is painted on it. `ProjectStore`
  re-establishes that sharing on load, so a reopened project is as cheap as a new one.
- **Dirty channels.** Each grid keeps three independent dirty sets — `Render` (needs GPU
  re-upload), `Store` (needs writing to SQLite), `Preview` (thumbnail stale). They are
  cleared by different consumers at different times, so one flag could not serve all three.

`core::memory::document_memory` measures what the document actually owns, counting each
allocation once **by address** so shared tiles are not double-counted. Reach for it before
claiming a memory win; it is served over FFI and shown in Settings.

### Camera and coordinates

`Camera` holds zoom, pan and viewport; `viewport.rs` adds culling (`visible_doc_rect`),
device sizing, and `paper_scissor` — the on-screen rect the paper occupies, in device
pixels, which is what every content draw is clipped to.

**Painting APIs take screen coordinates and convert once, in the engine.** No pan/zoom
arithmetic happens in Swift, ever. Zoom steps, the log zoom curve (`zoom_unit` /
`zoom_from_unit`), the fit padding, the min/max zoom rules and `is_fit` are all core
functions the shell reads results from.

The two ends of the zoom range are set independently and **share no constant** — `max_zoom`
used to be `min_zoom` times a factor, which meant lowering the floor silently lowered the
ceiling with it. The floor is `MIN_ZOOM_FILL` (0.2 of the viewport); the ceiling is whatever
puts `MIN_VISIBLE_DOC_SIDE` (16) document pixels across the short viewport side, under a
flat `MAX_ZOOM_HARD` of 64×, with a `.min(shorter_doc)` guard so a 16px document still
zooms. Do not re-derive one from the other.

### History

`History` (`core/src/history.rs`) is a stack of `HistoryCommand`s, each a set of `TileDiff`
(before-images of the tiles a stroke touched), `MaskDiff` and `RunDiff`, capped by
`HISTORY_MEMORY_BUDGET_BYTES` (256 MiB) rather than by step count — undo staying instant is
the point, and a step's cost is its pixels, not its existence. Because the before-images are
`Arc` clones of tiles that mostly did not change, a deep stack is far cheaper than its
nominal size.

Cold entries are **compacted, not dropped**: past `HISTORY_HOT_COMMANDS` from either end of
the stack, a uniquely-owned tile collapses to `HistoryTile::Uniform` (four bytes, and most of
a drawing app's history is flat or transparent) or to a zstd frame. The gate is
`Arc::strong_count == 1` rather than age, because a snapshot the live document still shares
costs history nothing — compressing it would force the copy the sharing was avoiding.
`HistoryCommand.bytes` tracks the compacted size, so `evict()` really does admit more
commands; a saving the budget cannot see would buy no undo depth at all.

Known limit: history is **tile/mask/run diffs only**. Structural edits — add/remove/
duplicate/merge layer, opacity, filters, `⌘T`, vector item edits — are deliberately outside
the model today. History also dies with the document; it is
not persisted.

### Strokes, shapes and selection

The paint pipeline is uniform:

```
pointer_down → begin_stroke / shape_drag
pointer_move → push_stroke_point   (GPU previews; nothing committed)
pointer_up   → CoverageGrid or Shape::coverage → blend into tiles → history step
```

`CoverageGrid` (`core/src/coverage.rs`) is why this is not "stamp discs straight into tiles".
A stroke's consecutive stamps overlap by half a radius; blending each one separately is
invisible at full opacity and ruinous below it — the stroke comes out a dark, beaded rope.
So coverage accumulates as a **maximum** into a sparse, tile-shaped scratch grid, and the
whole stroke composites onto the layer exactly once. The GPU preview does the same thing with
`Max` blending (see `stroke_coverage.rs` below), which is why the stroke does not change
appearance when the pointer comes up.

`Shape` (`core/src/shape.rs`) is geometry only — two endpoints, a half-width, and independent
`fill` / `stroke` flags — evaluated as a signed distance function. **The same `Shape` answers
where a selection rectangle is**, which is exactly why it carries no colors.

`Selection` is a *document*-level concept, not a layer or a mask: a `Rect`/`Ellipse`/`Lasso`
shape stored as parameters, or a `Mask` (one bit per pixel) for the magic wand and for
invert, which have no closed form. Everything downstream — paint clipping, copy, cut, delete
— goes through `bounds()` and `contains()`, which is why adding the `Mask` variant required
no changes anywhere else.

---

## 3. `render` — the board

This is the most intricate part of the engine, so it gets the most space. The contract:

> Swift owns the `MTKView` and the `CAMetalLayer`. Rust *borrows* the layer pointer (no
> retain), creates the wgpu surface from it, and owns everything else — pipelines, textures,
> buffers, the frame's decisions. **Nothing drawn on the board is a SwiftUI view**: paper,
> grid, strokes, handles, guides, marching ants and the layer hover outline are all WGSL.

### 3.1 Why the board is drawn the way it is

Three forces shaped this renderer:

1. **A document can be much larger than a screen.** 8K × 8K is 1024 tiles per layer. Anything
   that costs *per tile in the document* rather than *per tile on screen* is a bug.
2. **Editing is interactive and continuous.** A stroke, a slider drag, a pan — all of them
   run at display rate, and the mutex around the document is held for the whole frame. Work
   that can be skipped must be skipped, not merely made fast.
3. **The board and the exporter must agree.** A shape drawn live and the same shape flattened
   into a PNG have to be the same pixels. That is a *duplication* problem: the SDFs exist in
   both Rust and WGSL, and they are kept in lockstep by convention plus tests, not by codegen.

Everything below is a consequence of one of those.

### 3.2 The tile atlas

`TileAtlas` (`render/src/tile_atlas.rs`) is **one shared `texture_2d_array` holding every
GPU-resident tile across the whole document** — every layer pooled together, addressed by
array-layer index.

The reason is draw calls. With one texture per tile, every visible tile needs its own bind
group and its own `draw()`; a zoomed-out multi-layer document is thousands of draw calls a
frame. With a shared array, the texture binds once and a whole document layer's tiles become
a **single instanced draw**, with the per-tile origin and array index riding in an instance
buffer. On a weak integrated GPU, where per-draw-call overhead dominates, that is the whole
difference.

- Grown (never shrunk) by doubling, from `TILE_ATLAS_INITIAL_CAPACITY` (128) to
  `TILE_ATLAS_MAX_CAPACITY` (4096) — capped further by whatever
  `max_texture_array_layers` the adapter reports. A `wgpu::Texture` reserves VRAM for its
  full declared layer count, so a small document must never pay for a big array.
- Free slots are a **LIFO `Vec<u32>`** (`pop` / `push`). Every slot is the same size, so
  there is nothing to fragment; do not add a coalescing allocator.
- A 256×256 RGBA8 tile is **1024 bytes per row**, which is a multiple of wgpu's 256-byte
  copy alignment — `write_texture` can read the tile `Arc` with no CPU pad.
- Every slot carries a **full mip chain** (`compose::tile_mip_chain`). Without mips, a
  zoomed-out pan minifies raw 256×256 texels through a plain bilinear filter, and that
  aliasing *is* the shimmer. The opposite end has the opposite problem: past
  `CRISP_PIXEL_ZOOM` the board is *magnifying*, where a bilinear tap smears one texel into a
  gradient the width of the whole magnified pixel — so `fs_tile` swaps to a
  nearest-`mag_filter` sampler there and deep zoom shows pixels rather than a blur of them. The chain costs about a third more storage, so the real worst
  case is nearer 1.3 GiB than the 1 GiB the base levels alone suggest —
  `TileAtlas::capacity_bytes` accounts for it.
- When the atlas is full, `allocate` returns `None` and `sync_tiles` evicts — always
  preferring a **prefetch-margin tile** (retained just outside the viewport) over anything
  the viewport can actually see.

### 3.3 What gets uploaded, and when

`Renderer::sync_tiles` is the CPU-heavy half of a content frame:

1. Walk visible layers, collect the tiles inside the **retained** rect (visible expanded by
   `GPU_TILE_RETENTION_MARGIN_TILES` = 3 tiles, so small pans re-upload nothing) and mark
   which of those are dirty or missing.
2. **Bake and mip in parallel.** `composited_tile_payload` folds mask and
   opacity into a copy of the tile — returning `None`, and thus allocating nothing, for the
   common case of a layer with neither — and `tile_upload_mips` builds the chain. Both
   are pure pixel math that scales with tile count, so both go through `rayon`; only the
   `wgpu` calls stay sequential.
3. Upload, allocating or evicting atlas slots. A tile whose bytes were **not** baked can share
   an atlas slot with its siblings if they share an `Arc` — this is what keeps unpainted Paper
   at a single GPU tile.
4. Clear the `Render` dirty bit **only for tiles that actually reached the atlas**. Clearing
   it for a dropped upload strands the tile forever: the draw builder skips a tile with no
   slot, and nothing dirty would ever ask for it again — a permanent hole in the layer.

A **hidden layer keeps its atlas slots**. Dropping them would make the eye icon cost a full
re-upload — recomposite plus re-mip, seconds of stalled main thread on a deep document — on
the way back. Its tiles just stop counting as visible, which makes them the first thing
eviction gives up.

### 3.4 Instances and the draw list

Four instance types, all `#[repr(C)] + Pod`, all written straight into vertex buffers:

| Instance | Built by | Drawn by |
| --- | --- | --- |
| `TileInstance { origin, slot }` | `build_layer_draws` | `vs_tile` / `fs_tile` |
| `StrokeInstance { segment, color, brush }` | `compose::stroke_instances` and friends | `vs_stroke` / `fs_stroke` |
| `VectorShapeInstance { p0, p1, color, stroke_color, half_width, tool, fill, stroke }` | `vector_draw::shape_instance` | `vs_vector_shape` / `fs_vector_shape` |
| `GuideInstance` | `compose::guide_instances` | `vs_guide` / `fs_guide` |

`StrokeInstance` is drawn by **two** pipelines, and which one is the whole distinction
between ink and chrome. `vs_stroke` measures `brush.x` in *document* units, because a live
pen stroke, a lasso and a selection's marching ants are shaped like what they will commit
as. `vs_overlay` reads the same field as a *screen*-pixel half-width — transforming the
endpoints by `pu.zoom`/`pu.pan`, padding the quad in screen space, and evaluating
`sd_segment_pts` against screen coordinates exactly as `fs_guide` does — because board
furniture (the `⌘T` and vector-item frames, the text session's box and caret, the layer
hover outline) has to be the same size at every zoom. Both ride contiguous ranges of the
one stroke buffer, so the split costs a second `draw`, not a second upload. New chrome goes
on the overlay pass; only ink goes on the stroke pass.

`fs_overlay` has one branch: an instance carrying a non-zero **half height** (`brush.z`, which
`BrushProfile::HARD` leaves at zero, so every other piece of chrome is unaffected) is a filled
box rather than a capsule, evaluated with the same `sd_box` `shape_distance`'s `TOOL_RECT`
uses. That is how a text selection's rows are drawn, and it is the one piece of chrome whose
size *does* follow the zoom — the engine hands it `row_height * zoom * 0.5`, because a
highlight that stayed one screen size would stop covering the glyphs beneath it.

`build_layer_draws` walks the **whole layer stack once** and emits an ordered
`Vec<LayerDraw>`:

```rust
enum LayerDraw {
    Tiles(BlendMode, LayerId, Range<u32>),  // one instanced draw, any number of tiles
    Solid(BlendMode, LayerId),              // unpainted Paper: one document-sized quad
    Vector(VectorRun, Range<u32>),          // one shape, or one path's stroke segments
}
```

Two decisions live in that list:

- **Stack order is preserved across kinds.** Vector layers used to be drawn before every
  tile layer, which put them under Paper where nothing could be seen. Building one list
  across all layers means a vector layer above a paint layer covers it — exactly as the
  flattened composite already had it.
- **One draw per vector layer.** A layer holds one item, so `build_layer_draws` emits one
  `LayerDraw::Vector` and stops. There is no `extend_run` inside a layer, and adjacent
  vector layers are not coalesced — stack order and per-layer blend stay honest.

`draw_cached_content` replays that list into whatever attachment it is handed. Positions are
**document-space** in the instance buffers, so the same buffers reproduce correctly at any
camera state — nothing in the replay reads the document. That is precisely what lets the same
function serve both a full redraw and a scissored strip repair (§3.7).

### 3.5 Bind groups, uniforms, pipelines

Bindings are arranged by *how often they change*:

- **Group 0 (tiles):** `TileCamera` uniform + the atlas array texture + **two** samplers +
  the layer table — bound once for the whole board, shared by every tile and solid draw. The
  samplers (`TileSamplers`) differ only in `mag_filter`; `fs_tile` picks between them on
  `TileCamera::crisp`, a flag the engine sets from `limits::CRISP_PIXEL_ZOOM` so the renderer
  never re-invents the threshold.
- **There is no group 1 for tiles.** Everything that varies per *document layer* — pivot,
  offset, scale, rotation, solid Paper's atlas slot, opacity and the adjustment LUT (`tone`,
  `saturation`, `vibrance`, `lut_mode`; plan 23) — lives in one read-only storage buffer of
  `LayerData` rows at `@group(0) @binding(4)`, written once per content rebuild by
  `write_layer_data`. **Row *i* is `doc.layers[i]`**: a row index is a stack position, so a
  tile instance addresses its layer directly and no side table resolves it. Vector layers and
  hidden ones own a row nobody reads, which is cheaper than an index that means something
  different in each frame. The binding is `VERTEX_FRAGMENT`, not vertex-only: `vs_tile`/
  `vs_doc_quad` still read the transform, and `fs_tile`/`fs_solid_tile` now read opacity and
  the LUT to evaluate `apply_adjustments` per pixel instead of the CPU baking it into tile
  bytes before upload — one row, 1072 bytes, growing from the original 32.
- **`preview_bg`** is group 0 for everything overlay-shaped: strokes, guides, the shape
  preview, and vector shapes. One uniform block (`PreviewUniforms`) carries the camera plus
  the current tool/color/geometry, so an overlay pipeline needs no per-draw state at all.
- **`paper_bg`** is `PaperUniforms` plus one small texture: the baked desk lattice
  (`render/src/desk.rs`). The desk is screen-locked and periodic, so one `cell × dpr` square of
  two-channel coverage — red for the cell rules, green for the corner crosses — addressed by
  device pixel modulo that period reproduces the whole viewport. `fs_paper` reads it with
  `textureLoad` and an integer modulo rather than a sampler, so the texel a pixel lands on is
  exact arithmetic at any viewport coordinate. `PaperUniforms::lattice_side` carries the period,
  and **zero** puts the shader back on evaluating the pattern itself — the fallback for a
  backing scale where `cell * dpr` is not a whole number of texels and the lattice would drift
  out of phase. Rebuilt on a `dpr` change and never otherwise; the grid *colors* stay uniforms,
  so a theme switch is still a buffer write.

What the table bought is not bytes — it is that a stack of Normal layers draws with **one**
`set_bind_group` for the whole board. Before, every layer's instanced draw was preceded by a
rebind of its own one-uniform group, so a 40-layer document paid 40 of them per frame to say
things that mostly read `scale: [1, 1], offset: [0, 0]`. `TileInstance` grew a `layer_index`
into what was already padding, so the per-tile payload did not get bigger.

The unpainted-Paper quad (`vs_doc_quad`) has no instance buffer to carry an atlas slot, so its
row holds one and the draw names the row through its **instance range**: `draw(0..6, i..i+1)`,
read back as `@builtin(instance_index)`. That replaced a genuinely sharp edge — the slot used
to be bitcast into `LayerXform.pivot.x`, a union over one buffer that had two draw paths
reading the same bytes as different types.

Blend mode is still per *pipeline*, and vector layers still composite in stack order against
tiles, so the draw list is one instanced draw per contiguous run of the same pipeline. The
table did not remove those splits; it removed the bind that used to happen *inside* a run.

Pipelines (all from the single `board.wgsl` module):

| Pipeline | Purpose |
| --- | --- |
| `paper` | Fullscreen desk: baked grid lattice + paper border, screen-space |
| `tile_normal` / `tile_multiply` / `tile_screen` | Tile draws, one per blend mode |
| `solid_normal` / `solid_multiply` / `solid_screen` | The unpainted-Paper quad, same three |
| `stroke` | Stroke capsules in document units: live pen, lasso, marching ants |
| `overlay` | The same capsules measured in screen pixels: transform and item frames, the text box and caret, the hover outline |
| `guide` | Ruler guides |
| `shape` | The live shape-drag preview (fullscreen triangle, SDF per pixel) |
| `vector_shape` | Committed parametric vector items |
| `stroke_coverage` ×2 | Offscreen coverage accumulate + composite (§3.8) |
| `blit` / `clear_transparent` | `PanCache` quad and strip clear (§3.7) |
| `overview` | The zoomed-out proxy quad (§3.6) |

**Blend mode is a pipeline, not a uniform**, because Multiply and Screen need the destination
framebuffer — they are blend-state choices the fragment shader cannot make. Mask and opacity
only ever read the source layer's own pixels, so they are baked on the CPU at upload time
and cost no shader at all.

Everything composites in **premultiplied alpha**. `fs_tile` returns `rgb * a`, the blend
states are `One / OneMinusSrcAlpha`, and even the mip downsampler weights each tap by its own
alpha before averaging — because a fully transparent neighbour is not "no color", it is
color nobody sees yet, and averaging straight RGB lets it bleed.

### 3.6 The shader

One file: `render/src/shaders/board.wgsl`, ~21 entry points, validated by **naga at build
time** in `render/build.rs` — a malformed shader is a failed `cargo build`, not a black
window.

Two rules govern it:

1. **Never branch on bare literals.** `tool == TOOL_LINE`, not `tool == 1u`, with the
   constants matching `calumma_core::Tool`'s discriminants.
2. **The board and the exporter evaluate the same distance function.** `shape_region` /
   `shape_ink` in WGSL mirror `Shape::region_distance` / `fill_distance` / `stroke_distance`
   plus `ink_sample` in Rust — same SDF, same half-pixel antialiasing band, same fill-under-
   stroke compositing order. When one changes, the other has to. This duplication is
   deliberate (the GPU cannot call Rust and the exporter cannot call a shader), and it is the
   single most fragile invariant in the renderer.

### 3.7 Three ways to produce a frame's content

The renderer never draws tile and vector instances straight into the swapchain. Content goes
into `PanCache` (`render/src/framebuffer.rs`) — two viewport-sized offscreen color textures
with **fixed roles**, not an alternating ping-pong:

- **`reference`** holds the last full content redraw, plus the exact pan/zoom/dpr/scissor it
  was drawn at.
- **`working`** is rebuilt from `reference` on a camera-only frame.

Each frame picks one of three modes, then the board pass draws a single textured quad
sampling whichever texture the content pass produced:

| Mode | When | Cost |
| --- | --- | --- |
| **Reuse reference** | Nothing content-shaped moved and the camera matches the reference — i.e. every overlay-only frame: a pen stroke mid-gesture, a shape being dragged out, a blinking caret | Zero. The content pass is skipped entirely |
| **Shift + patch** | Camera-only pan at unchanged zoom | One `copy_texture_to_texture` plus a scissored redraw of the up-to-four exposed edge bands |
| **Full redraw** | Anything else — content changed, zoom changed, first frame, or the shift is ineligible | The whole visible stack |

The shift is measured in **whole device pixels** and the result is promoted to be the next
frame's reference, which keeps the exposed bands at one frame's worth of travel — a few
pixels on a normal drag — rather than growing with the gesture. Each band is cleared to
transparent before repainting (`fs_clear_transparent`), because `LoadOp::Load` preserves the
freshly copied region and a semi-transparent stroke there would otherwise blend against
pixels from two frames ago.

`shift_plan` and `exposed_rects` are pure rect arithmetic and are unit-tested with no GPU
device at all (`render/tests/framebuffer.rs`) — the reason that math lives in its own module.

**The overview path** (`render/src/overview.rs`, policy in `overview_lod.rs`) is the other
content strategy, for when the board is zoomed far out: past `OVERVIEW_ENTER_TILE_THRESHOLD`
(48) visible tiles, tile sync is skipped and the paper is one textured quad. The texture is a
level from a 4-step pyramid, finest cap from `GpuBudget::overview_finest_side()` (4096 at rest
on a standard device, 2048 low-tier / Warn, 1024 Critical), coarsest 256. Pan is a uniform
write; zoom picks a different level. A paint re-flattens only the 1024-doc-px chunks it
touched (`DirtyChannel::Overview`); a stack change rebuilds the displayed level. Hysteresis
exits at 24. It is disabled while a gesture is live, since the proxy cannot show what is being
drawn. See `docs/RENDERING.md` § Overview path.

Known gap, honestly stated in `RENDERING.md`: content is composited into `PanCache` from
transparent and blended over the desk afterwards. That is identical for Normal layers, but a
Multiply/Screen layer that is the *bottom-most visible* layer blends against transparent
instead of the desk pattern. Paper is the bottom layer in every ordinary document.

### 3.8 The stroke coverage pass

`render/src/stroke_coverage.rs` is the GPU twin of `core/src/coverage.rs`, and exists for the
same reason. A live brush stroke is one capsule per recorded point pair, and consecutive
capsules overlap almost entirely when the pointer moves slowly. Alpha-blending them straight
onto the board composites the same ink over itself dozens of times.

So the capsules render into a single-channel `R8Unorm` target with **`Max` blending** — union,
not sum — and the board gets one composite of the finished shape, tinted by the stroke ink in
`fs_stroke_composite`. Both halves accumulate the same maximum, which is why the stroke looks
identical before and after pointer-up. The target is allocated the first time a brush stroke
needs one, so a session that never paints never pays for it.

**It accumulates across frames rather than being rebuilt each one.** `Max` is idempotent and
order-independent, so unioning segment *N* onto the union of `0..N` equals unioning `0..N+1`
from empty — which means a frame only has to draw the capsules the pointer actually travelled
since the last one. `Renderer` keeps a `CoverageProgress` (stroke generation, point count,
camera, brush params, ink) and hands `stroke_instances_from` the tail; `accumulate`'s `restart`
flag then loads what is already there instead of clearing the viewport. Rebuilding the whole
stroke every frame — which is what this used to do, full-viewport clear included — made a live
stroke cost O(points) per frame and O(points²) over the gesture, so the brush got heavier the
longer the line got.

Restarting is the only way coverage comes *out* of the target, so anything that invalidates what
is in it has to be caught: a new stroke, a camera that moved under pixels measured in device
space, a changed brush width or ink, a resized target, and the one-point degenerate capsule that
segment 0 replaces rather than follows. The subtle one is **`Document::stroke_generation`**,
which bumps not only per `begin_stroke` but whenever `push_stroke_point` *rewinds* the list —
a Shift-held straight segment truncates back to its anchor on every event, and `Max` cannot take
the abandoned capsule back out. The contract that number carries is exactly: while it holds,
`stroke_points` is an append-only extension of what it was.

### 3.9 Motion mode

Entered on any camera invalidation, left after four idle frames or on pointer-up. While
active it drops the per-tile mip build (base level only), skips the preview uniform and
stroke rebuild on camera-only frames, and reuses a cached visible-tile count. Tiles uploaded
base-only are tracked in `base_only_tiles` and re-uploaded in full once the camera settles —
otherwise zooming out samples a mip level nobody ever wrote and the layer fades out.

Motion mode deliberately **does not touch the swapchain** any more. `SURFACE_FRAME_LATENCY`
is 1, set once for the life of the surface, because `Surface::configure` drains the entire
GPU queue — and that reconfigure used to land inside the first `mouseDragged` of every pan.

For the per-frame ordering, the dirty-flag state machine, and the optimization roadmap, go to
[`docs/RENDERING.md`](RENDERING.md).

---

## 4. `ops` — AI and heavy operations

A tiny crate that exists to keep one decision out of the shell: **whether an operation runs
in Rust or on the platform.**

```rust
trait Op { fn kind(&self); fn backend(&self); fn available(&self) -> bool; fn run(...); }
```

`OpRegistry` holds a core map and a platform map. `resolve` prefers the platform
implementation **when it reports `available()`**, otherwise falls back to core, otherwise the
op is unavailable and the UI greys it out. The shell asks for `OpKind::RemoveBackground`; it
never learns that macOS answered with Vision.

`apply_output` is the other half: an op returns an `OpOutput` (a mask, a raster, or paths) and
the engine — not the shell — decides what that means for the layer stack and pushes the
history step. The shell never edits the stack after an op.

Shipped today: Remove Background, platform-only, via `VNGenerateForegroundInstanceMaskRequest`.
The other `OpKind` slots exist with registry tests and no implementations, on purpose.

---

## 5. `io` — persistence and export

### SQLite

One database (`ProjectStore`, `io/src/store.rs`) at the OS-native app-data directory resolved
through the `dirs` crate — never a hardcoded path. WAL journaling, foreign keys on.

```
projects(id, name, width, height, created_at, opened_at, thumb, accent, guides)
layers(project_id, layer_id, name, visible, z_index, mask, content_kind,
       vector_data, opacity, blend_mode, adjustments, text_data, transform, locked)
tiles(project_id, layer_id, tx, ty, pixels)
open_project_tabs(position, project_id)
```

**`open_project_tabs` is the tab bar**, one row per open project, `position` ordering them.
It replaced three `workspace*` tables from when projects were grouped into workspaces and the
titlebar tabs switched *those* — see "Workspaces are gone" in `AGENTS.md`.

**Tiles are rows, not a blob.** That is what makes a save incremental: only tiles whose
`Store` dirty bit is set are written, so painting one corner of an 8K document writes a
handful of rows rather than the document. On load, `tile::uniform_color` re-detects solid
tiles and re-shares one `Arc` across them, so a reopened project has the same copy-on-write
economy a fresh one does.

`ProjectStore::open` creates the schema above in one `CREATE TABLE IF NOT EXISTS` batch — the
single, always-current source of truth for what's on disk, no separate migration layer behind
it. (Pre-2026-09 builds ran additive `PRAGMA table_info` + `ALTER TABLE` migrations here to
carry an existing installed base forward a column at a time; dropped once there was no real
installed base yet to stay compatible with — see the deleted `docs/plans/05-single-db-schema-
script.md`.) Blob-format evolution is a separate, still-live mechanism — see Blobs below.

### Blobs

The non-pixel per-layer data each has its own small binary codec, one file each:
`vector_blob`, `text_blob`, `adjustments_blob`, `transform_blob`, `guides_blob`. They carry a
**version word** and decode older versions rather than rejecting them — `vector_blob` is at
v3 and still reads v1 (paths only, before shapes were parametric) and v2 (one color per item,
before fill and stroke were independent), mapping the old data onto the new shape on read.
Adding a field means bumping the version and writing the migration in the same change.

### Export

- **PNG / JPEG / WebP / AVIF / HEIC** — `Document::composite_rgba` flattens the visible stack
  respecting masks, opacity, blend modes and adjustments, and hands raw RGBA over FFI; the
  shell encodes it through ImageIO. `io/src/png.rs` is the engine's own PNG codec, used where
  the engine needs bytes itself: clipboard copy, project thumbnails, and the rasters embedded
  inside an SVG export.
- **PSD** (`io/src/psd.rs`) — layered.
- **SVG** (`io/src/svg.rs` + `core/src/vector_svg.rs`) — layered, and a vector item emits the
  matching SVG *primitive* (`<rect>`, `<ellipse>`, `<path>`) rather than a flattened polyline,
  so the export stays as editable as the layer is. This is the payoff of storing parameters.
- **PDF** (`io/src/pdf.rs` + `core/src/vector_pdf.rs`) — layered, and the closest fit of the
  three: `layer.opacity` is `/ca`/`/CA`, the blend modes are `/BM` names that map one for one,
  and a shape emits real path operators. Two things it does that the others cannot: a painted
  layer's alpha rides a separate `/SMask` image (PDF images have no alpha channel), and the
  page carries one flip matrix rather than negating every y. A hand-written writer, for the
  same reason `svg.rs` is — the container is a header, numbered objects, an xref table and a
  trailer. `io/src/flate.rs` supplies `/FlateDecode`, since PDF's one general filter is zlib
  and the undo stack's zstd is no help here.
- Import is flattened composite only.

---

## 6. `text`

A leaf over `cosmic-text`. Three things worth knowing:

- `fonts.rs` resolves installed families **once** into a sorted, case-folded registry that
  also records which bold/italic cuts each family really ships. `family_exists` is a binary
  search, and `set_text_family` can refuse a name nothing can shape.
- Caret questions are answered against the **shaped layout, never the string**. A wrapped
  paragraph is one `BufferLine` laid out as several rows, so `layout.rs` picks the row by
  glyph byte range rather than by line index, and horizontal steps go through cosmic-text
  `Motion` so one press crosses a whole grapheme cluster.
- `raster.rs` produces RGBA that `core::text_layer::resync` blits into the layer's tile cache.

A typing session is **one** undo step: tiles and run are snapshotted when it opens and a
single `TileDiff` + `RunDiff` lands when it closes. Per-keystroke history would flood the
budget for no benefit.

---

## 7. `ffi` — the boundary

The only crate Swift links, via `platform/macos/Calumma/Bridge/Calumma.h`.

### Rules

- **`Inner` is behind a `parking_lot::Mutex`** and holds the document, the store, the
  renderer, the op registry and the coalesced input state. Ops can therefore run off the main
  thread.
- **Every entry point goes through `with_inner`**, which null-checks the pointer, takes the
  lock, and wraps the call in `catch_unwind`. A Rust panic becomes `CalmStatus::Error`; it
  never unwinds into Swift. There is a read-only counterpart for getters that return a value
  instead of a status.
- **`unsafe` is expected here and nowhere else.** Null checks, `CStr`, `Box::from_raw`,
  `create_surface_unsafe`, the platform vtable. Keep it thin, keep the helpers centralised,
  and do not copy the patterns up into `core`/`render`/`ops`.
- **The header and the Swift `Engine` wrapper are not cross-checked.** Adding an FFI function
  means editing `Calumma.h` and `Engine.swift` in the same change, or the build links against
  a symbol that is not there.

### What crosses the boundary

- `CalmState` — one `#[repr(C)]` struct read once per frame: size, zoom, min/max zoom, pan,
  active layer, layer count, undo/redo availability, accent, `zoom_unit`, last shape/select
  tool, `is_fit`. Every derived value the chrome shows is computed in core and *reported*, not
  recomputed in Swift.
- Pixels in and out — premultiplied RGBA on the way in (unpremultiplied in Rust),
  heap-allocated buffers on the way out with an explicit free function.
- The platform op vtable (`platform.rs`) — three function pointers, each call wrapped in
  `catch_unwind` so a Swift-side throw cannot cross back into Rust unwinding.

### Two things that do not run on the frame

- **Pan coalescing.** `calm_engine_pan` and the scroll entries do not render. They accumulate
  deltas into `Inner` and mark the renderer camera-dirty; the next `MTKView` frame calls
  `flush_pending_camera` and draws once. Input arrives faster than the display refreshes, and
  drawing per event is wasted work.
- **Autosave.** `AutosaveThread` (`ffi/src/autosave.rs`) is spawned in `calm_engine_new` and
  joined in `calm_engine_free` before the mutex drops. It wakes on a condvar every
  `AUTOSAVE_INTERVAL_MS` (800 ms) and takes the lock with **`try_lock`**, skipping on
  contention and forcing only after `AUTOSAVE_MAX_SKIPPED_TICKS` consecutive skips. Moving
  SQLite off the render path was only half the fix — a blocking lock on a background thread
  just relocates the stall into an arbitrary point in a frame. A skipped tick costs 800 ms of
  staleness; a blocked frame is visible.

### Leaving a document

Every path out of an open document — closing it, switching tabs, opening another — must
route through `Inner::close_document`, which saves, drops the document, and calls
`Renderer::release_document` so the GPU textures and the atlas slots keyed by its layer ids
go with it. Add a new way to leave and forget this, and its tiles stay in VRAM where
nothing will ever evict them: `sync_tiles` only runs with a document open.

---

## 8. Invariants a change must not break

1. `core` compiles without wgpu/objc/metal/SQL. (`./manage.py purity`)
2. Coordinate math, clamping, camera, history, tile math and product constants live in Rust.
   Swift renders what the engine reports.
3. `Shape::distance` in Rust and `shape_region`/`shape_ink` in WGSL describe the same shape.
4. Masks, opacity, blend modes, adjustments and transforms are never baked into tile bytes.
5. Sparse stays sparse; history does not deep-copy layers; GPU uploads stay dirty-region
   scoped.
6. A tile's `Render` dirty bit is cleared only when its upload actually reached the atlas.
7. Text tiles are a cache; the run is what is stored.
8. Nothing unwinds across the FFI boundary.
9. Every exit from a document goes through `close_document`.
10. On-disk blob formats are versioned and decode their predecessors.

---

## 9. Working here

```
./manage.py test      # cargo test --workspace
./manage.py lint      # clippy + ruff + purity
./manage.py check     # fmt + lint + test
./manage.py coverage  # llvm-cov, per-crate table
./manage.py dev       # build ffi, xcodegen, open Xcode
```

Tests live in `engine/<crate>/tests/<module>.rs` — one file per module under test, not in
`#[cfg(test)] mod tests` blocks inside the source. `cargo test` already treats each file as
its own integration crate against the library's public API, so logic files stay logic.
High coverage is expected on `core` (camera, tiles, history, shapes, paint commit) and on the
`ops` registry; the GPU path is covered where the math can be lifted out of it
(`framebuffer.rs`'s rect arithmetic is the model to follow).

`AGENTS.md`'s "no comments" rule is about *narrating* code — a comment restating what the
next line does. Explaining **why** is the opposite, and the engine does it heavily: module
docs (`//!`), item docs (`///`), and a block above anything whose shape is a decision rather
than an obligation. Most of the reasoning summarised in this README lives there in more
detail. Read them.
