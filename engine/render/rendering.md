# rendering.md — how the board is drawn

Companion to `AGENTS.md` (architecture) and `FLOW.md` (product flow), living inside the
crate it documents (see `AGENTS.md`'s "Canvas / render" section). Describes the GPU path,
dirty flags, and the pan/zoom performance strategy. The engine owns all of this; Swift only
owns the `MTKView` surface and calls FFI.

---

## Frame loop

```
MTKView draw (60–120 Hz, display-linked)
  └─ Engine.flushPendingState()     // zoom pill, pan readout — once per frame max
  └─ calm_engine_render
       ├─ autosave()                // SQLite, throttled (800 ms), skipped while stroking
       ├─ flush_pending_camera()    // coalesced pan/scroll deltas from FFI
       └─ Renderer::render(doc)
            ├─ early-out if Clean && no live preview
            ├─ decide overview vs tile path
            ├─ sync GPU tiles / overview texture (if needed)
            ├─ rebuild draw lists (if needed)
            ├─ write uniforms (paper, tile camera, preview — camera-only skips preview)
            └─ one Metal render pass: desk → paper content → overlays
```

Input during pan does **not** call `render()` directly. `calm_engine_pan` queues deltas and
marks the renderer **camera-dirty**; the next `MTKView` frame flushes and draws.

---

## Dirty flags

### `FrameDirty` (renderer)

| State | Set by | Meaning |
| --- | --- | --- |
| `Clean` | end of `render()` when nothing live-editing | Skip entire frame unless `has_live_preview()` |
| `Camera` | `invalidate_camera()` — pan, scroll, zoom, fit, resize | Camera moved; tile pixels unchanged |
| `Content` | `invalidate()` — paint, layer edit, theme, document load | Tiles, vectors, or overlays may have changed |

`invalidate_camera()` also enters **motion mode** (see below). Content invalidation clears
cached tile-draw counts and marks the overview texture dirty.

### Tile `DirtyChannel` (document)

Per-tile dirty sets on each `TileGrid`:

| Channel | Cleared when | Purpose |
| --- | --- | --- |
| `Render` | GPU upload in `sync_tiles` | Tile needs (re)composite + atlas upload |
| `Store` | SQLite save | Tile bytes changed on disk |

Mask/adjustment/opacity changes mark tiles render-dirty without mutating tile bytes.

### Shell `stateDirty` (Swift)

Pan/zoom call `syncStateSoon()` instead of `syncState()` so SwiftUI does not diff the whole
editor on every mouse-drag event. `flushPendingState()` runs inside `draw()` and reads
`calm_engine_state` once per frame.

### Save `dirty_save` (FFI `Inner`)

Set on document edits; `autosave()` may write SQLite inside `render()` when the 800 ms
interval has elapsed and no stroke is active.

---

## Two content paths

### Tile path (default)

1. **`sync_tiles`** — for each visible layer, upload dirty tiles into a shared
   `texture_2d_array` atlas (one array layer per tile). CPU work: mask/adjustment bake +
   mip chain (skipped in motion mode).
2. **`build_layer_draws`** — walk the layer stack; emit `LayerDraw` entries (tiles, solid
   paper quad, vector runs).
3. **Draw** — instanced quads per tile layer; vectors as stroke/shape instances.

Retention: tiles within `GPU_TILE_RETENTION_MARGIN_TILES` (3) of the visible rect stay
GPU-resident even when off-screen, so small pans do not re-upload.

### Overview path (zoomed out)

When visible tile count ≥ **48** (exit at **24**), skip tile sync entirely:

1. **`composite_overview`** — CPU flatten of the document to ≤ 2048 px RGBA (on open
   prewarm + on content dirty).
2. **One textured quad** inside the paper scissor — pan/zoom = uniform updates only.

Disabled while live-editing (stroke, shape preview, text caret).

---

## Motion mode (fast pan)

Entered by `invalidate_camera()` / `begin_camera_motion()`. Exited by
`end_camera_motion()` (mouse-up after pan) or **4 idle frames** without new camera input.

While active:

| Change | Why |
| --- | --- |
| `desired_maximum_frame_latency = 1` | Less display buffering |
| Tile upload: base mip only | Skip 8-level mip build on CPU |
| Camera-only frames: skip preview uniform + stroke clone | Less CPU + buffer writes |
| Cached visible tile count | Skip recount for overview hysteresis |

---

## Paper solid quad

Unpainted Paper (`whole_tiles_share_one_arc()`) uploads **one** atlas tile and draws a
single document-sized quad (`vs_doc_quad`). Any paint on a whole tile forks the shared
`Arc` → falls back to per-tile instances.

---

## Render pass order (one pass)

1. **Fullscreen desk** (`fs_paper`) — grid + paper border outside scissor logic
2. **Paper scissor set** — clip to on-screen paper bounds
3. **Content** — overview quad *or* layer draw list (tiles / solid / vectors)
4. **Overlays** — live stroke, selection, transform handles, text caret (skipped on
   camera-only frames)

Clear colour is black; desk fills the viewport.

---

## What still costs on a camera-only pan

Even after Tier A optimizations:

- `autosave()` check every frame (cheap unless interval elapsed)
- Full framebuffer **clear** every frame
- Desk fullscreen triangle (simplified in motion mode)
- Uniform writes: paper + tile camera (+ overview camera if active)
- `get_current_texture` + present
- Mutex: entire `render()` holds `Inner`

---

## Optimization roadmap

See `plans/07-display-cache.md` for the full Figma-style display-cache plan
(todo #7). Tier B items below are folded into that plan's phases.

See the tier list at the end of this file. Highest leverage next steps are
**framebuffer scroll-blit** (phase 1) and **decoupling autosave from the
render thread** (phase 0).

---

## Tier B — next high-impact (recommended)

| # | Change | Effect | Throw away? |
| --- | --- | --- | --- |
| B1 | **Framebuffer scroll / ping-pong blit** on camera-only pan: copy previous frame with offset, redraw only exposed strips | Biggest Figma-like win; pan becomes ~2 blits + edge repair | No — additive |
| B2 | **Move autosave off render path** — background thread or timer, never inside `calm_engine_render` | Removes mutex + SQLite from frame budget | No |
| B3 | **Skip desk clear on camera-only** — `LoadOp::Load` + blit previous colour attachment, or persistent desk texture | Saves full-screen fill | No |
| B4 | **Lower overview enter to ~32** once prewarm is reliable | More 8K pans hit overview sooner | Slight quality trade at mid zoom |
| B5 | **R8 or RGB10A2 desk** if banding acceptable | Less memory bandwidth on fill | Minor visual |

## Tier C — medium

| # | Change | Effect | Throw away? |
| --- | --- | --- | --- |
| C1 | **Separate tile path entirely during motion** — never rebuild draw list; only uniforms | Already partial; finish by skipping `visible_needs_gpu_upload` checks on camera-only | No |
| C2 | **GPU compositing for adjustments** instead of CPU bake per dirty tile | Slider drag on large docs | CPU path for export stays |
| C3 | **Layer flatten cache** — one GPU texture per layer at rest, patch on edit | Fewer instances when many layers | Memory ↑ |
| C4 | **Display link driven render** — `isPaused = true`, draw only when dirty | No idle 120 Hz wakeups | Requires explicit `setNeedsDisplay` wiring |
| C5 | **Read zoom pill from atomics** — `flushPendingState` only when chrome visible | Less Swift publish per frame | No |

## Tier D — simplify / throw out

Things you can remove or gate behind quality settings if smooth pan matters more:

| Remove or defer | Savings | Product cost |
| --- | --- | --- |
| **Procedural desk grid** at rest (static texture or no grid until zoom > 1) | Shader ALU every pixel | Desk looks flatter when idle |
| **Multiply / Screen blend modes** on tile path (Normal only live) | 2 of 3 tile pipelines + blend state churn | Feature loss |
| **Per-tile mip chain** at rest (bilinear only, accept shimmer when zoomed far out) | CPU on upload, VRAM × ~1.33 | Moiré when heavily zoomed out |
| **Layer transform GPU path** for pan frames | Uniform + shader branch per layer | Transformed layers wrong during fast pan until settle |
| **Vector live stack** during overview | Overview already flattens vectors | Vectors invisible until zoom in |
| **120 Hz MTKView** — cap at 60 during pan | Half frame work | Slightly less smooth on ProMotion |
| **Retention margin 3 → 1** when not in motion | Less atlas pressure | More upload churn on pan stop |

---

## Figma comparison (honest)

Figma's smoothness comes from a **different contract**:

- Infinite canvas with **scene graph** + **cached tiles** at multiple fixed zoom levels
- Pan often **translates existing pixels** (scroll blit), not re-rasterize
- **No full-scene CPU composite** on the hot path
- **No SQLite** on the display thread
- Aggressive **level-of-detail** — text, effects, and grid degrade during motion

Calumma is closer to a **pixel editor** (sparse tiles, undo, masks, adjustments). Matching
Figma on pan is achievable; matching Figma on *everything* without a scene-graph rewrite
is not. The pragmatic target: **pan/zoom feels like Figma; edit fidelity stays like Krita**.

---

## Key files

| Path | Role |
| --- | --- |
| `engine/render/src/renderer.rs` | Frame loop, dirty flags, sync, draw lists |
| `engine/render/src/overview.rs` | Overview texture LOD |
| `engine/render/src/tile_atlas.rs` | Shared GPU tile array |
| `engine/render/src/shaders/board.wgsl` | Desk, tiles, overview, solid quad, vectors |
| `engine/render/src/compose.rs` | CPU tile bake, mips, overlay instances |
| `engine/ffi/src/engine.rs` | Pan coalescing, `calm_engine_render` |
| `platform/macos/.../BoardCanvas.swift` | `MTKView` delegate, input |
| `engine/core/src/limits.rs` | Thresholds (overview, retention, latency) |
