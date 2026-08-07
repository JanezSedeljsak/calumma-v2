# AGENTS.md

Calumma is a personal whiteboard: bounded project canvases you draw on with a pen or
shapes. Multiple projects, switched from the window titlebar tabs; each switch clean-loads from SQLite.

**Ambition:** product depth and scale in the neighbourhood of GIMP, Photoshop, Krita, and
Figma — multi-layer documents, large canvases, dense interaction. Performance and
scalability are first-class constraints on every change, not afterthoughts. The chrome
stays clean and minimalistic (`design/STYLE.md`); complexity lives in the engine, not in
cluttered UI.

**Read this file first, then `FLOW.md` and `design/STYLE.md`.** Follow the one rule
below before inventing architecture. Prefer extending what exists over adding parallel
systems.

---

## The one rule

**The engine owns all state and all compute. The shell owns nothing but UI knobs.**

Shell knobs only: active tool, colour, brush size, shape fill, panel visibility, open tab
ids, theme, **language**. Coordinate math, clamping, pixels, camera, history, ops dispatch,
and board visuals live in Rust/WGSL.

If you are about to do pan/zoom arithmetic, tile math, or layer-stack mutation in Swift —
stop. Call an FFI method instead and keep the logic in `engine/`.

---

## Repository map

| Path | Role |
| --- | --- |
| `engine/core` | Document, sparse tiles, camera, history, shapes, `LayerContent` — no GPU |
| `engine/render` | wgpu; surface created by the shell; applies layer masks at upload |
| `engine/io` | SQLite projects + encode/decode |
| `engine/ops` | `Op` / `OpRegistry` dispatch; apply results into the document |
| `engine/ffi` | C ABI; **only** crate Swift links; platform op vtable |
| `platform/macos` | SwiftUI landing, tabs, editor chrome, Metal canvas, Vision ops, i18n loader |
| `translations/` | Locale JSON (`en.json` today). Not code — edit strings here |
| `design/` | Visual tokens only (`tokens.json`), `STYLE.md`, SVG icons |
| `FLOW.md` | Product flow: screens, canvas, shortcuts, I/O |
| `cli/` | Python helpers + leaf tools (`_helpers.py`, tokens, purity, …) |
| `manage.py` | Task runner (Python 3.14). Prefer this over Make. |

Dependency direction:

```
core  ← std + small utils only
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
   from `Tokens.generated.swift`. Do not sprinkle one-off fonts/colours/padding.
4. **After `design/tokens.json` edits:** `./manage.py tokens`.
5. **After Rust engine edits that affect the app:** `./manage.py test` (and rebuild ffi /
   open Xcode via `./manage.py dev` when touching the shell).
6. **No comments** in `.rs`, `.swift`, `.wgsl`. Name things clearly instead.
7. **Do not edit generated** `Tokens.generated.swift` by hand.

---

## Projects and navigation

- DB path: `~/Library/Application Support/Calumma/calumma.sqlite`.
- Landing: name + resolution, presets from tokens, recents list, hero pane.
- Editor: **titlebar** project tabs (right of traffic lights). Switch = save/close current → open selected (full
 reload, no preload of inactive docs).
- One board per project; bounded paper (not infinite canvas). Zoom out floor fills ~50% of
  the viewport; zoom in max is 10× that floor (detail-capped around a 400px visible side).

---

## Layers

```rust
pub enum LayerContent {
    Raster(TileGrid),           // sparse 256×256 RGBA tiles
    Vector(Vec<VectorPath>),    // document-space paths; compositing later
}
```

- No per-layer transform. Paths/tiles live in document space.
- Optional `layer.mask: Option<Vec<u8>>` (full-document coverage 0–255). Masks do **not**
  mutate tile bytes; the renderer multiplies alpha when uploading GPU tiles.
- Vector compositing is not finished — filled closed paths (e.g. Paper) render; other vector work is deferred.

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
space, type, colour, presets). Engine name constants live in `calumma_core::names`. CLI
paths/binaries live in `cli/constants.py`.

Compose `CalmText`, `CalmField`, `CalmRow`, `calmSurface()`, `CalmChip`, etc. Theme colours
via `@Environment(\.themeColors)`; copy via `@Environment(\.l10n)`.

1. No stroke borders — background contrast only.
2. Minimal corner radius (`Tokens.Radius.sm` / `md`) on controls and islands.
3. Custom Canvas/`AppIcon` drawings only — no icon packs / SF Symbols as product icons.
4. Light and dark from tokens; tell the engine dark-paper via FFI.
5. Filled controls; hover = luminance shift.
6. Native `ColorPicker` on macOS.
7. No SwiftUI overlays on the Metal board (hover outline is a shader).

Details: `design/STYLE.md`.

---

## Performance and scalability

This is a drawing tool aimed at pro-app workloads (large boards, many layers, long
sessions). Prefer speed carefully — measure, then optimise. Not a secrets vault, but also
not a place to pile micro-opts that muddy the code for single-digit percent gains.

- Live strokes/shapes preview on the GPU; CPU commits on pointer-up into sparse tiles.
- Dirty-flag rendering; idle board submits nothing.
- Tile pixels are `Arc` COW; history shares unchanged tiles (`Arc::make_mut` on write).
- Cap history by memory budget; design for documents that outgrow a single bitmap.
- Painting APIs take **screen** coordinates; convert once in the engine.
- Engine `Inner` is behind a `Mutex` so ops can run off the main thread.
- Scalability checklist on structural changes: sparse tiles stay sparse, history does not
  deep-copy whole layers, GPU uploads stay dirty-region scoped, ops do not block the
  render loop longer than necessary.

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

- Viewport-sized Metal surface; paper positioned by camera matrix in WGSL.
- Swift owns the `MTKView` / CAMetalLayer; Rust borrows the layer pointer (no retain).
- Layer hover = dashed outline in the shader, not a Swift overlay.

### WGSL naming

Never branch on bare literals (`tool == 1u`). Use named consts matching Rust
(`TOOL_LINE`, …) and `switch`. Keep discriminants aligned with `calumma_core::Tool`.

---

## Testing and tooling

```
./manage.py tokens       # design/tokens.json → Swift Tokens
./manage.py test         # cargo test --workspace
./manage.py coverage     # llvm-cov + per-crate %% table in the log
./manage.py lint         # clippy + purity
./manage.py check        # fmt + lint + test
./manage.py purity       # core has no platform/GPU deps
./manage.py dev          # build ffi, xcodegen, open Xcode
```

Expectations:

- High coverage on `engine/core` (camera, tiles, history, shapes, paint commit).
- `engine/ops` registry tests: platform beats core, `available()` gating, failed ops leave
  the document untouched.
- Pre-commit: fmt, clippy, swift-format, no-comments, purity.

`cli/_helpers.py` holds shared paths, cargo helpers, and design-token accessors. Leaf tools
under `cli/` import from it. `manage.py` is the CLI entrypoint.

Cargo workspace root is `engine/Cargo.toml` (rustfmt/clippy live there). Swift format
config is `platform/macos/.swift-format` only.

Pin versions in `[workspace.dependencies]`. Never `*` or bare `^`.

---

## Deliberately deferred

Workspaces-as-products beyond SQLite projects, vector compositing polish, BiRefNet /
`ort`, GenerateTexture model manager, SuggestShape, Vectorize (`vtracer`), image import,
PSD/PDF, text layers, eyedropper, region select — add only as considered features, not by
restoring old app code.
