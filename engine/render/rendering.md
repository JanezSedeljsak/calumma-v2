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
       ├─ flush_pending_camera()    // coalesced pan/scroll deltas from FFI
       └─ Renderer::render(doc)
            ├─ early-out if Clean && no live preview
            ├─ decide overview vs tile path
            ├─ sync GPU tiles / overview texture (if needed)
            ├─ rebuild draw lists (if needed)
            ├─ write uniforms (paper, tile camera, preview — camera-only skips preview)
            ├─ content pass: PanCache full redraw, or shift + patch on a camera-only frame
            └─ one Metal render pass: desk → PanCache blit quad (or overview) → overlays
```

Input during pan does **not** call `render()` directly. `calm_engine_pan` queues deltas and
marks the renderer **camera-dirty**; the next `MTKView` frame flushes and draws.

Autosave no longer lives on this path — see "Autosave" below.

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
| `Render` | GPU upload in `sync_tiles` **succeeded** | Tile needs (re)composite + atlas upload |
| `Store` | SQLite save | Tile bytes changed on disk |

Mask/adjustment/opacity changes mark tiles render-dirty without mutating tile bytes.

`Render` is cleared per tile only for uploads that actually reached the atlas. When the atlas
is at `TILE_ATLAS_MAX_CAPACITY` and every live tile is on screen, there is nothing to evict and
the upload is dropped; clearing that tile's dirty bit anyway stranded it — `build_layer_draws`
skips a tile with no atlas slot, and nothing would ever ask for it again — so the layer kept a
permanent hole showing through as bare paper.

### What counts as a live preview

Two predicates, because "needs another frame" and "needs another *content* frame" are not the
same question.

`Document::has_live_preview()` means a **gesture** is in flight — the pointer is down and board
geometry is being dragged out under it: an active stroke, a shape drag, a transform or vector
drag. It forces the overview proxy off and blocks a shifted pan-cache blit, because neither is
valid for a frame whose content is moving.

`Document::has_animated_overlay()` means an overlay is animating on the renderer's own clock
with nothing about the document changing. Only the blinking text caret. It pins `frame_dirty`
to `Overlay`, not `Content`.

A **hovered layer is deliberately in neither**. Its outline is a static overlay, and
`calm_engine_set_hover_layer` already calls `invalidate()` on the way in and on the way out,
which is exactly the one frame it needs. Counting the hover as live made resting the cursor on
a layer row resync every tile at 120 Hz *and* disable the overview proxy — on precisely the
documents that are too large to draw the full way.

An **active selection** and **transform mode** are in neither, for the same reason. Both are
modes you sit in rather than gestures you perform — a marquee lives until ⌘D — and both draw
static overlays. Counting them as live pinned `Content` at display rate for as long as the mode
was open: every tile resynced, the draw list rebuilt, the whole visible stack recomposited, the
overview proxy off. Every FFI entry that touches either (`calm_engine_deselect`,
`calm_engine_toggle_transform`, `calm_engine_exit_transform`, the pointer commits) already
calls `Renderer::invalidate`, which is the one frame they need.

Nothing pins `Content` any more. `render()` ends a frame on `Overlay` while a gesture or the
caret is live and `Clean` otherwise; content invalidation comes from the events that actually
change content. `calm_engine_pointer_move` asks `Document::pointer_move` which kind it was — a
pen or a shape drag lays no pixels down until pointer-up, so those frames are overlay-only,
while the blur brush (which commits mid-drag), a layer move and a transform/vector drag all
return `true` and invalidate content.

### Shell `stateDirty` (Swift)

Pan/zoom call `syncStateSoon()` instead of `syncState()` so SwiftUI does not diff the whole
editor on every mouse-drag event. `flushPendingState()` runs inside `draw()` and reads
`calm_engine_state` once per frame.

### Save `dirty_save` (FFI `Inner`)

Set on document edits; `Inner::autosave` writes SQLite when the 800 ms interval has elapsed
and no stroke is active. It runs on a dedicated background thread (`engine/ffi/src/
autosave.rs`), not inside `calm_engine_render` — see "Autosave" below.

---

## Autosave

`AutosaveThread` (`engine/ffi/src/autosave.rs`) is spawned in `calm_engine_new` and stopped
(signal + join) in `calm_engine_free`, before the `Inner` mutex is dropped. It wakes every
`AUTOSAVE_INTERVAL_MS` on a condvar (so `calm_engine_free` doesn't wait out a full interval to
tear down) and calls the same `autosave()` the render path used to call inline. Nothing about
`autosave()` itself changed — dirty-flag check, stroke-active guard, the 800 ms throttle — only
*what calls it*.

Moving the call off the render path was only half of it, and an earlier version of this file
overstated the result: both threads still contend for the one `Mutex<Inner>`, so a blocking
lock on the autosave thread only *relocated* the stall — `calm_engine_render` would wait out a
whole SQLite transaction at an arbitrary point in a frame, with no back-pressure. The tick now
takes the lock with `try_lock` and skips on contention, forcing the lock only after
`AUTOSAVE_MAX_SKIPPED_TICKS` consecutive skips so a continuously-drawn document still reaches
disk. A skipped tick costs 800 ms of staleness; a blocked frame is visible.

---

## Two content paths

### Tile path (default)

1. **`sync_tiles`** — for each visible layer, upload dirty tiles into a shared
   `texture_2d_array` atlas (one array layer per tile). CPU work: mask/adjustment bake +
   mip chain (skipped in motion mode).
2. **`build_layer_draws`** — walk the layer stack; emit `LayerDraw` entries (tiles, solid
   paper quad, vector runs).
3. **Content pass** — `draw_cached_content` replays those `LayerDraw` entries into the
   `PanCache` reference texture (`engine/render/src/framebuffer.rs`), not the swapchain
   directly. On a camera-only frame with zoom/dpr/viewport unchanged from that reference,
   `sync_tiles`/`build_layer_draws` are skipped entirely (as before) and the content pass
   instead shifts the reference into the `PanCache` working texture by the rounded device-pixel
   pan delta (`copy_texture_to_texture`) and redraws only the exposed edge strip(s)
   (`framebuffer::exposed_rects`) — see "PanCache" below.
4. **Board pass** — draws a single textured quad sampling whichever `PanCache` texture the
   content pass produced, instead of the tile/vector instances directly.

Retention: tiles within `GPU_TILE_RETENTION_MARGIN_TILES` (3) of the visible rect stay
GPU-resident even when off-screen, so small pans do not re-upload.

### Overview path (zoomed out)

When visible tile count ≥ **48** (exit at **24**), skip tile sync entirely:

1. **`composite_overview`** — CPU flatten of the document to ≤ 2048 px RGBA (on open
   prewarm + on content dirty).
2. **One textured quad** inside the paper scissor — pan/zoom = uniform updates only.

Disabled while live-editing (stroke, shape preview, text caret).

---

## PanCache (scroll-blit)

Phase 1 of `plans/07-display-cache.md`. `PanCache` (`engine/render/src/framebuffer.rs`) is two
fixed-role offscreen color textures, sized to the viewport — not an alternating ping-pong:

- **`reference`** holds the last full content redraw (every visible tile/vector draw call,
  scissored to the paper rect) and the exact pan/zoom/dpr/scissor it was drawn at. It is only
  ever replaced by another full redraw.
- **`working`** is rebuilt from `reference` every camera-only frame: `copy_texture_to_texture`
  shifts `reference` by the rounded device-pixel delta between `reference`'s pan and the
  current one, then `exposed_rects` computes the up-to-four bands the copy could not have
  populated (the edges the shift slid away from) and `draw_cached_content` repaints just those,
  each cleared to transparent first (`fs_clear_transparent`) so a semi-transparent stroke there
  cannot blend against two-frames-old pixels.

Blitting always measures the shift from the same frozen `reference` pan rather than chaining
frame to frame, so per-frame rounding to whole device pixels cannot accumulate into visible
drift over a long pan gesture. `PanCache::plan` is the eligibility gate: no reference yet, or
zoom/dpr changed since it was captured, or the shifted overlap is empty (a jump too large, or a
corner case at a viewport edge) all fall back to a full redraw that frame instead of blitting.
`shift_plan`/`exposed_rects` are pure rect arithmetic, unit-tested without a GPU device in
`engine/render/tests/framebuffer.rs`.

The board pass never draws tile/vector instances directly on a camera-only frame — it draws one
textured quad (`vs_blit`/`fs_blit`) sampling whichever `PanCache` texture the content pass
produced. Desk and the paper border still redraw every frame (`fs_paper`, screen-space, cheap);
only the tile/vector content — the part that scales with document size — goes through
`PanCache`. The overview path (see above) is unaffected; it keeps drawing its own quad.

**Known gap:** content is composited into `PanCache` starting from a transparent texture, then
blended over the desk in the board pass — mathematically identical to compositing progressively
over desk from the start for Normal-alpha layers, but a Multiply/Screen layer that is the
*bottom-most visible* layer (Paper hidden or fully erased) now blends against transparent
instead of against the desk pattern. Paper is the bottom layer in the overwhelming common case,
where this does not apply; fixing the edge case would mean baking desk into the shiftable
texture, which would make the (deliberately screen-locked, non-scrolling) desk grid pan with
the content instead.

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

## Render pass order

Two passes now, both inside the same `wgpu::CommandEncoder`:

**Content pass** (skipped entirely when `use_overview`) — draws or shifts+patches into a
`PanCache` texture; see "PanCache" above.

**Board pass**, into the swapchain:

1. **Fullscreen desk** (`fs_paper`) — grid + paper border outside scissor logic
2. **Paper scissor set** — clip to on-screen paper bounds
3. **Content** — overview quad, *or* one `PanCache` blit quad (`vs_blit`/`fs_blit`)
4. **Overlays** — live stroke, selection, transform handles, text caret (skipped on
   camera-only frames)

Clear color is black; desk fills the viewport.

---

## Swapchain queue depth

`SURFACE_FRAME_LATENCY` is 1 and is set once, at surface configuration, for the life of the
surface. It used to be 2 at rest and 1 during motion, flipped by a `set_frame_latency` helper
that called `Surface::configure` — and wgpu drains the entire GPU queue before it will
reconfigure a surface (`Device::configure_surface` polls with `PollType::wait_indefinitely`).
`begin_camera_motion` is reached from `calm_engine_pan`, which the shell calls synchronously
from `mouseDragged`, so that put a full pipeline stall on the main thread inside the first drag
event of every pan gesture, and another one four idle frames after the last. A bursty
scroll-wheel pan crossed that boundary several times a second.

Motion mode still exists and still does the things worth doing — base-mip-only uploads, skipped
preview uniform and stroke rebuild on camera-only frames, cached visible tile count. It just no
longer touches the swapchain.

## What still costs on a camera-only pan

After Tier A **and** shipped Tier B1/B2 (below):

- Desk fullscreen triangle every frame (simplified in motion mode; not blitted — see PanCache's
  known gap above for why)
- `PanCache` blit quad: one `copy_texture_to_texture` + a scissored draw per exposed strip,
  bounded by how far the camera moved that frame, not by document size
- Uniform writes: paper + tile camera (+ overview camera if active)
- `get_current_texture` + present
- Mutex: entire `render()` holds `Inner` (autosave no longer competes for it mid-frame)

---

## Optimization roadmap

See `plans/07-display-cache.md` for the full Figma-style display-cache plan (todo #7).
Tier B1 (framebuffer scroll-blit) and B2 (autosave off the render thread) are **shipped** —
phases 0 and 1 of that plan. The rest of Tier B, and Tiers C/D, remain open.

---

## Tier B — next high-impact (recommended)

| # | Change | Effect | Throw away? | Status |
| --- | --- | --- | --- | --- |
| B1 | **Framebuffer scroll / ping-pong blit** on camera-only pan: copy previous frame with offset, redraw only exposed strips | Biggest Figma-like win; pan becomes ~2 blits + edge repair | No — additive | **Shipped** — `PanCache`, see above |
| B1b | **Reuse the `PanCache` reference on an overlay-only frame** — no shift, no redraw, the board pass samples it directly | Brush strokes, shape drags and the caret stop recompositing the visible stack per frame | No — additive | **Shipped** — `reference_matches` / `reuse_reference` |
| B2 | **Move autosave off render path** — background thread or timer, never inside `calm_engine_render` | Removes mutex + SQLite from frame budget | No | **Shipped** — `engine/ffi/src/autosave.rs` |
| B3 | **Skip desk clear on camera-only** — `LoadOp::Load` + blit previous color attachment, or persistent desk texture | Saves full-screen fill | No | Open |
| B4 | **Lower overview enter to ~32** once prewarm is reliable | More 8K pans hit overview sooner | Slight quality trade at mid zoom | Open |
| B5 | **R8 or RGB10A2 desk** if banding acceptable | Less memory bandwidth on fill | Minor visual | Open |

## Tier C — medium

| # | Change | Effect | Throw away? |
| --- | --- | --- | --- |
| C1 | **Separate tile path entirely during motion** — never rebuild draw list; only uniforms | Already partial; finish by skipping `visible_needs_gpu_upload` checks on camera-only | No |
| C2 | **GPU compositing for adjustments** instead of CPU bake per dirty tile — `plans/23-gpu-adjustment-evaluation.md` (LUT + opacity on the `LayerData` SSBO from `plans/02-strict-scope-optimizations.md`) | Slider drag on large docs | CPU path for export stays |
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
- Pan often **translates existing pixels** (scroll blit) — Calumma now does this too for a
  camera-only frame at unchanged zoom, via `PanCache`, though only single-level (no pyramid)
- **No full-scene CPU composite** on the hot path
- **No SQLite** on the display thread — Calumma now matches this too (autosave is background)
- Aggressive **level-of-detail** — text, effects, and grid degrade during motion

Calumma is closer to a **pixel editor** (sparse tiles, undo, masks, adjustments). Matching
Figma on pan is achievable; matching Figma on *everything* without a scene-graph rewrite
is not. The pragmatic target: **pan/zoom feels like Figma; edit fidelity stays like Krita**.
Phases 2+ of `plans/07-display-cache.md` (a real chunk pyramid, multi-level LOD) are what
would close the remaining gap.

---

## Key files

| Path | Role |
| --- | --- |
| `engine/render/src/renderer.rs` | Frame loop, dirty flags, sync, draw lists |
| `engine/render/src/framebuffer.rs` | `PanCache` — scroll-blit reference/working textures, shift + exposed-rect math |
| `engine/render/src/overview.rs` | Overview texture LOD |
| `engine/render/src/tile_atlas.rs` | Shared GPU tile array |
| `engine/render/src/shaders/board.wgsl` | Desk, tiles, overview, solid quad, vectors, `PanCache` blit/clear |
| `engine/render/src/compose.rs` | CPU tile bake, mips, overlay instances |
| `engine/ffi/src/engine.rs` | Pan coalescing, `calm_engine_render` |
| `engine/ffi/src/autosave.rs` | Background autosave thread |
| `platform/macos/.../BoardCanvas.swift` | `MTKView` delegate, input |
| `engine/core/src/limits.rs` | Thresholds (overview, retention, latency) |
