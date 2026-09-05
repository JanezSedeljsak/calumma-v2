# RENDERING.md — how the board is drawn

Companion to `AGENTS.md` (architecture) and `docs/FLOW.md` (product flow); the crate it
documents is `engine/render` (see `AGENTS.md`'s "Canvas / render" section). Describes the GPU path,
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
cached tile-draw counts. Tile writes already mark `DirtyChannel::Overview`; a stack change
(visibility, opacity, blend, mask, transform, adjustments, vector bounds) is detected on the
next overview sync via a stamp, not by flattening the whole document from `invalidate()`.

### Tile `DirtyChannel` (document)

Per-tile dirty sets on each `TileGrid`. `mark_dirty` inserts the coord into **every** channel
so a write is never silent on one of them:

| Channel | Cleared when | Purpose |
| --- | --- | --- |
| `Render` | GPU upload in `sync_tiles` **succeeded** | Tile needs (re)composite + atlas upload |
| `Store` | SQLite save | Tile bytes changed on disk |
| `Preview` | Layer-thumbnail rebuild | Cached preview pixels are stale |
| `Overview` | After `OverviewPass` flattens the chunks that tile belongs to | Pyramid levels re-flatten only those 1024-doc-px chunks |

Mask/adjustment/opacity changes mark tiles render-dirty without mutating tile bytes. They
rebuild the overview through the stack stamp, not through this channel.

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
with nothing about the document changing. Only the blinking text caret.

It no longer pins `frame_dirty` at all. The caret is a square wave — `TEXT_CARET_BLINK_SECONDS`
is 1.06 — and pinning `Overlay` ran the whole board pass at display rate for as long as a text
session was open: on a ProMotion panel, 120 frames a second to service a signal that changes
state twice. `render()` instead compares `compose::text_caret_visible` against the phase of the
frame it last actually drew (`drawn_caret_phase`) and returns early while that answer has not
moved, so a parked cursor costs two frames a second. The gate and the drawing call the *same*
function on purpose: a gate that disagreed would drop the frame meant to show the flip. The
comparison also catches the caret going away, which needs one frame to erase it.

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

Nothing pins `Content` any more, and only a live *gesture* pins `Overlay`. `render()` ends a
frame on `Overlay` while a gesture is live and `Clean` otherwise; content invalidation comes
from the events that actually change content. `calm_engine_pointer_move` asks `Document::pointer_move` which kind it was — a
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

## Frame pacing

The `MTKView` runs free (`isPaused = false`, `enableSetNeedsDisplay = false`) at whatever the
screen it is on can do. What it draws at is now the engine's call: `calm_engine_frame_hint`
answers frames per second, or **0** for "as fast as the display allows", and
`Coordinator.draw(in:)` assigns it to `preferredFramesPerSecond`. The ceiling stays the shell's
(`BoardMTKView.displayCeiling`, from `NSScreen.maximumFramesPerSecond`); the engine names only
the floor it can live with, and `applyFrameRate` takes the min.

`Renderer::frame_hint` returns the display maximum whenever something is in flight — a gesture
(`has_live_preview`), a camera still settling (`camera_motion`), a text session, or a frame the
renderer has already marked dirty for itself. Otherwise `FRAME_HINT_IDLE_FPS` (10): a board
sitting still has nothing waiting on the display link, and 120 wakeups a second for a picture
that is not moving is the whole of what idle costs.

A `DeviceTier::Low` machine gets `FRAME_HINT_LOW_TIER_FPS` (60) in place of the display maximum
— a GPU that cannot hold 120 gains nothing from being asked to try, and pacing it at a rate it
can actually hold is what makes a gesture feel even. On a 60 Hz display the clamp is a no-op.

**Latency.** `BoardMTKView.wake()` puts the rate straight back to the ceiling from every event
that can arrive while the board is idle. Without it the first frame after a rest waits out an
idle interval before the engine gets to report that something is happening — which is exactly
the frame the pointer is waiting on. `wake()` only ever speeds the view *up*; the engine still
owns when it may go quiet. This is deliberately smaller than C4 below (`isPaused` plus explicit
`setNeedsDisplay` wiring) and is not thrown away by it: `frame_hint` returning 0 is the natural
way to say "pause" if a wakeup per interval ever turns out to be too much.

---

## Device tier

`calumma_core::DeviceTier` is classified once in `Renderer::from_surface` from the adapter —
`GpuKind` (mapped from `wgpu::DeviceType`) plus `max_texture_array_layers`. A software adapter
is `Low` outright; an integrated one is `Low` only when it *also* reports no more than the
WebGPU downlevel baseline of 256 array layers. Apple Silicon reports `IntegratedGpu` with a
large limit and stays `Standard`: the tier must not punish the machine the app is built on.

| Knob | `Standard` | `Low` |
| --- | --- | --- |
| Retention margin | `GPU_TILE_RETENTION_MARGIN_TILES` (3) | 1 |
| Atlas ceiling | `TILE_ATLAS_MAX_CAPACITY` | ÷ 4 |
| Frame-hint ceiling | display maximum | 60 |
| `wgpu::MemoryHints` | `Performance` | `MemoryUsage` |

**A tier is a floor; memory pressure is a ceiling.** Both want the retention margin and the
atlas ceiling, so neither may set them directly — `GpuBudget` holds the fixed tier and the live
`PressureState` and answers with the **stricter** of the two. `set_memory_pressure` reads its
new capacity back out of the budget rather than off the level, so a report that recovered all
the way to `Normal` cannot hand a weak GPU a ceiling it never had.

`PowerPreference` is deliberately left at `HighPerformance`. On a dual-GPU Intel Mac it forces
the discrete part for a workload whose hot path is CPU→GPU tile uploads — free on unified
memory, a bus copy on a discrete one — so `LowPower` is arguable there. Only arguable: the
integrated part it would pick instead is genuinely weaker on fill rate, and it is not measurable
on the machine this is developed on. Deciding it by guess would risk making things worse on
exactly the machines the tier exists to help.

---

## Desk lattice

`fs_paper` is a fullscreen triangle on every frame the board renders at all, and its grid used
to be evaluated per pixel: two divisions, two `floor`/`round` pairs, six comparisons and two
`mix`es, at every pixel of a 4K viewport, every frame.

It does not have to be. `desk_pattern` reads **only** `screen` — the desk is deliberately
screen-locked, so it does not scroll with the board and does not scale with zoom — and both
halves of it repeat with period `DeskMetrics::cell`, anchored at the viewport origin. So
`render/src/desk.rs` bakes one period into a `cell × dpr` square `Rg8Unorm` texture (red = on a
cell rule, green = on a corner cross) and `fs_paper` reads it with `textureLoad` at
`device_pixel % side`. Integer modulo rather than a sampler and UVs: the texel a pixel lands on
is then exact arithmetic at any viewport coordinate, instead of a fraction that has to survive
being scaled up and wrapped back down.

Coverage stays a hard 0 or 255 and the two channels stay separate, so the shader performs
exactly the two `mix`es it always did against the theme's own `grid` color and alpha — the grid
*colors* are still uniforms, and a theme switch is still a buffer write. `render/tests` checks
the two paths byte-for-byte over a whole viewport at 1x, 1.5x and 2x.

`PaperUniforms::lattice_side` is 0 where `cell * dpr` is not a whole number of texels, which
puts the shader back on the procedural path — a period that does not tile would drift out of
phase across the viewport, and on a hard-edged grid that is visible. The condition is on the
*product*: a 26pt cell tiles at 1x, 1.5x and 2x alike. Rebaked on a `dpr` change and never
otherwise, at ~2.7 KiB.

The paper border band is untouched: it depends on pan and zoom, and it is four comparisons.

---

## Two content paths

### Tile path (default)

1. **`sync_tiles`** — for each visible layer, upload dirty tiles into a shared
   `texture_2d_array` atlas (one array layer per tile). CPU work: mask bake + mip chain
   (skipped in motion mode). Adjustments and opacity no longer bake here — see C2 below;
   `fs_tile`/`fs_solid_tile` evaluate them per pixel off the `LayerData` row instead.
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

When visible tile count ≥ **48** (exit at **24**), skip tile sync entirely and draw one
textured quad inside the paper scissor. Disabled while live-editing (stroke, shape preview,
text caret).

The quad samples a **pyramid**, not a single flatten. `OverviewPass` (`engine/render/src/
overview.rs`, level policy in `overview_lod.rs`) keeps up to `OVERVIEW_LEVELS` (4) GPU textures
at half-scale steps. The finest cap comes from `GpuBudget::overview_finest_side()` — 4096 on a
standard device at rest (`OVERVIEW_FINEST_SIDE`), 2048 on a low-tier GPU or under Warn pressure
(`OVERVIEW_MAX_SIDE`), 1024 under Critical. The coarsest floor is `OVERVIEW_COARSEST_SIDE`
(256). Which level is displayed is the coarsest whose `max_side` still covers
`doc_long * zoom * dpr`. Flattening is `Document::composite_overview_rect` in core, so a
chunked write and a full rebuild share one sampler.

Edits do not re-flatten the document. Tile writes mark `DirtyChannel::Overview` (independent of
`Render`, so zooming back into tiles still uploads). Those tiles map onto 1024-doc-px chunks
(`OVERVIEW_CHUNK_TILES` = 4) and only those rectangles are composited into the displayed
level — and remembered on the others until they are shown. A stack change (visibility, opacity,
blend, mask, transform, adjustments, vector bounds) mismatches a stamp and rebuilds the
displayed level in full. Memory pressure ≥ Warn drops the textures that are not on screen.

The enter/exit gate is still a **sum across layers**. That is leftover from the single-level
path, measured 2026-08-25, and is why a 10-layer 8K document can stay on the overview out to
the zoom cap even though the pyramid would now have a sharp enough level. Making the threshold
per-layer is the remaining cheap knob (todo #01); it is not required for the pyramid to pick
the right resolution once the path is on.

Historical single-level numbers, kept so the magnification trap is not rediscovered:

| Document (1600×1000 @2x) | Overview holds until | A 2048px flatten was then magnified |
| --- | --- | --- |
| 8192px, 1 painted layer | 1.57x zoom | 12.6x |
| 8192px, 3 painted layers | 3.16x zoom | 25.3x |
| 8192px, 10 painted layers | never, out to the 64x hard cap | — |
| 4096px, 1 painted layer | 1.57x zoom | 6.3x |

The display-cache plan that used to own a different fix (todo #7) was cancelled on 2026-08-25:
it was written against a renderer that no longer exists, and replacing the tile path with a
chunk atlas is pointless now that that path is bounded at 48 draws behind one bind group.

---

## PanCache (scroll-blit)

`PanCache` (`engine/render/src/framebuffer.rs`) is two
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

`visible_needs_gpu_upload` is memoized across frames on the same basis (`visible_upload_needed`,
cleared by `invalidate`, `sync_tiles` and `release_document`). It is only *reached* on a frame
where nothing dirtied content and the retained tile span did not move — a pan inside a single
tile — which is exactly the frame where the previous answer is still the answer. This is C1
below.

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

- Desk fullscreen triangle every frame — not blitted, see PanCache's known gap above for why.
  It is now one texel fetch and two `mix`es per pixel rather than the branchy per-pixel lattice
  math (see Desk lattice); it has never been simplified in motion mode, and an earlier version
  of this file said it was
- `PanCache` blit quad: one `copy_texture_to_texture` + a scissored draw per exposed strip,
  bounded by how far the camera moved that frame, not by document size
- Uniform writes: paper + tile camera (+ overview camera if active)
- `get_current_texture` + present
- Mutex: entire `render()` holds `Inner` (autosave no longer competes for it mid-frame)

---

## Optimization roadmap

Tier B1, B1b, B2 and C5 are **shipped**; B3 and C4 were investigated 2026-09-04 and not
implemented (see their table rows — B3 needs real new infrastructure for a cost already
measured as cheap, C4's remaining wakeup is already near-free inside the model this row's own
alternative would have to tear down). B5 and C3 remain open, both low-priority. The
display-cache plan that used to carry the roadmap beyond them (todo #7) was cancelled on
2026-08-25 — see the Overview path section for what came out of it and what is actually left.

**Where the headroom actually is, as of 2026-08-25.** The per-frame *draw* path is close to
done and the remaining items on it are small:

- A camera-only pan is a texture copy plus up to four scissored strips, bounded by how far the
  camera moved rather than by document size. An overlay-only frame does not even shift — it
  samples the reference directly.
- A content redraw is capped at **48 tile instances** by the overview threshold, and since the
  `LayerData` table landed it runs behind **one** `set_bind_group` for the whole board. Merging
  the remaining per-layer draws into one instanced draw per contiguous Normal run is now
  possible (instances blend in submission order, and the buffer is already filled in stack
  order) — but it would save on the order of ten draw calls a frame, so it is a curiosity, not
  a win. The tile path has nothing meaningful left in it.
- What is left on the CPU per frame is small and known: the `visible_needs_gpu_upload` walk
  (C1, 3–34 µs), the desk fullscreen triangle (B3/D), and uniform writes.

The overview path's single-level 2048px flatten — the fidelity problem measured 2026-08-25 —
is gone; see Overview path for the pyramid. The enter/exit gate now reads the busiest single
layer rather than summing tiles across the stack (`Renderer::busiest_layer_tile_count`,
shipped 2026-09-04 as todo #01) — a document with many sparse layers no longer gets stuck on
the overview at a zoom the pyramid could draw sharply.

---

## Tier B — next high-impact (recommended)

| # | Change | Effect | Throw away? | Status |
| --- | --- | --- | --- | --- |
| B1 | **Framebuffer scroll / ping-pong blit** on camera-only pan: copy previous frame with offset, redraw only exposed strips | Biggest Figma-like win; pan becomes ~2 blits + edge repair | No — additive | **Shipped** — `PanCache`, see above |
| B1b | **Reuse the `PanCache` reference on an overlay-only frame** — no shift, no redraw, the board pass samples it directly | Brush strokes, shape drags and the caret stop recompositing the visible stack per frame | No — additive | **Shipped** — `reference_matches` / `reuse_reference` |
| B2 | **Move autosave off render path** — background thread or timer, never inside `calm_engine_render` | Removes mutex + SQLite from frame budget | No | **Shipped** — `engine/ffi/src/autosave.rs` |
| B3 | **Skip desk clear on camera-only** — `LoadOp::Load` + blit previous color attachment, or persistent desk texture | Saves full-screen fill | No | **Investigated 2026-09-04, not implemented.** `desk_pattern` (the lattice) is a pure function of *screen* position — genuinely cacheable, no shift-and-patch needed unlike `PanCache`'s content. But `fs_paper`'s paper-border ring reads `(screen - pan) / zoom`, so it moves every pan frame; a correct version needs the lattice baked once into a persistent texture plus the border redrawn on top each frame, not a one-line `LoadOp` swap. That's real new infrastructure (a texture, resize/theme invalidation, a second pipeline) to save a cost already measured as "one texel fetch and two `mix`es per pixel" (see below) — not confirmed worth it without a frame-time trace, which nothing here can take |
| B4 | ~~**Lower overview enter to ~32**~~ — **withdrawn 2026-08-25**, then **superseded 2026-09-04** by the overview pyramid (#02) and the per-layer gate (#01) — see Overview path below | — | — | Superseded |
| B5 | **R8 or RGB10A2 desk** if banding acceptable | Less memory bandwidth on fill | Minor visual | Open |

## Tier C — medium

| # | Change | Effect | Throw away? |
| --- | --- | --- | --- |
| C1 | ~~**Separate tile path entirely during motion**~~ — **shipped**, plan 29. The `visible_needs_gpu_upload` walk (3 µs at 1 layer, 34 µs at 10) is memoized across the frames where its answer cannot have moved; see Motion mode above | — | — |
| C2 | ~~**GPU compositing for adjustments** instead of CPU bake per dirty tile~~ — **shipped 2026-09-02** as plan `23`. LUT + opacity moved onto the `LayerData` table (see `docs/ENGINE.md` § Bind groups); `fs_tile`/`fs_solid_tile` evaluate them per pixel via `apply_adjustments` | Slider drag on large docs | CPU path for export/flatten/pick stays (`AdjustmentLut`) |
| C3 | **Layer flatten cache** — one GPU texture per layer at rest, patch on edit | Fewer instances when many layers. Note the instance count is already bounded at 48 by the overview threshold, so this is only worth it *inside* a pyramid rebuild, not for the live tile path | Memory ↑ |
| C4 | **Display link driven render** — `isPaused = true`, draw only when dirty | No idle 120 Hz wakeups | Requires explicit `setNeedsDisplay` wiring. **Largely obviated** by plan 29's `calm_engine_frame_hint` (see Frame pacing), which drops idle to 10 fps and the caret from 120 full board passes a second to 2, without the wiring. **Investigated 2026-09-04, not implemented**: the remaining idle wakeup is already a cheap early-out (`render()`'s `Clean && no live preview` check) inside the standard `MTKView.preferredFramesPerSecond` polling model; removing it entirely needs exactly the `isPaused`/manual-`setNeedsDisplay` rearchitecture this row already argued against — wiring a redraw trigger into every state-mutating path for a wakeup that already does almost nothing |
| C5 | **Read zoom pill from atomics** — `flushPendingState` only when chrome visible | Less Swift publish per frame | No | **Shipped 2026-09-04** — `BoardCanvas.Coordinator.draw(in:)` skips `flushPendingState()` (and so the FFI round trip + `@Published state` republish) when `view.window?.occlusionState` says the window isn't visible; `engine.render()` still runs so the frame-hint throttle keeps working. Scoped to *window* visibility, not the zoom pill specifically — `state` backs more than the pill (layer count, active layer, tool gate), so gating on the pill's own SwiftUI visibility would have starved those too |

## Tier D — simplify / throw out

Things you can remove or gate behind quality settings if smooth pan matters more:

| Remove or defer | Savings | Product cost |
| --- | --- | --- |
| ~~**Procedural desk grid** at rest~~ — **shipped** as a baked one-period lattice, plan 29. No product cost at all: the drawn result is byte-identical (see Desk lattice) | Shader ALU every pixel | None |
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
Figma on pan is achievable — and largely done; matching Figma on *everything* without a
scene-graph rewrite is not. The pragmatic target: **pan/zoom feels like Figma; edit fidelity
stays like Krita**. Zoomed-out LOD is a pyramid now (see Overview path); what remains on that
path is the still-summed enter/exit gate, not a missing level.

---

## Key files

| Path | Role |
| --- | --- |
| `engine/render/src/renderer.rs` + `renderer/` | `Renderer` struct in `renderer.rs`; its `impl` split by concern across `renderer/pipeline.rs` (device/pipeline setup, blend states), `renderer/camera_motion.rs` (motion mode, visible/retained span), `renderer/cache.rs` (tile/layer cache, mip heuristics), `renderer/invalidation.rs` (dirty flags, buffer capacity), `renderer/frame.rs` (`sync_tiles`, draw-list build, `render()` itself) — same split-`impl` pattern as `viewport.rs`/`Camera` |
| `engine/render/src/framebuffer.rs` | `PanCache` — scroll-blit reference/working textures, shift + exposed-rect math |
| `engine/render/src/desk.rs` | Baked desk lattice — one period, two coverage channels |
| `engine/render/src/overview.rs` | Overview pyramid GPU pass — per-level textures, chunk uploads |
| `engine/render/src/overview_lod.rs` | Level sides, pick, 1024-px chunks, stack stamp |
| `engine/render/src/tile_atlas.rs` | Shared GPU tile array |
| `engine/render/src/shaders/board.wgsl` | Desk, tiles, overview, solid quad, vectors, `PanCache` blit/clear |
| `engine/render/src/compose.rs` | CPU tile bake (mask only, since C2), mips, overlay instances |
| `engine/ffi/src/engine.rs` | Pan coalescing, `calm_engine_render` |
| `engine/ffi/src/autosave.rs` | Background autosave thread |
| `platform/macos/.../BoardCanvas.swift` | `MTKView` delegate, input |
| `engine/core/src/device_tier.rs` | `DeviceTier` / `GpuBudget` — the tier floor combined with the pressure ceiling |
| `engine/core/src/limits.rs` | Thresholds (overview, retention, latency, frame hints) |
