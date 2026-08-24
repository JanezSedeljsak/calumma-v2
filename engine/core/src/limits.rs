pub const HISTORY_MEMORY_BUDGET_BYTES: usize = 256 * 1024 * 1024;
/// How many of the most recent commands on each stack are never compacted. An immediate
/// undo/redo round trip is the one history access that has to be instant, so the commands
/// either side of the cursor keep their raw bytes and pay no decompression.
pub const HISTORY_HOT_COMMANDS: usize = 8;
/// zstd level for cold history tiles. 1, not the default 3: the sweep runs on the autosave
/// tick while holding the engine lock, so wall time bounds how long a frame can be delayed —
/// and level 1 already beats LZ4's ratio on tile data for a fraction of the cost of chasing
/// the last few percent.
pub const HISTORY_COMPRESSION_LEVEL: i32 = 1;
/// Tiles one sweep may compact before yielding. Compaction happens under the engine lock, so
/// this is a latency bound rather than a throughput one — the rest is picked up on the next
/// tick, and a stack that never goes cold never queues any work at all.
pub const HISTORY_COMPACT_TILES_PER_SWEEP: usize = 16;

pub const EFFECT_CHUNK_BYTES: usize = 64 * 1024;

pub const STROKE_POINT_CAPACITY: usize = 256;
pub const MIN_STROKE_POINT_DISTANCE: f32 = 0.5;
pub const STAMP_SPACING_RATIO: f32 = 0.5;
pub const MIN_STAMP_SPACING: f32 = 0.5;
pub const STAMP_COVERAGE_PADDING: f32 = 1.0;

pub const BRUSH_SIZE_MIN: f32 = 1.0;
pub const BRUSH_SIZE_MAX: f32 = 96.0;
pub const BRUSH_SIZE_DEFAULT: f32 = 3.0;

/// Blur brush strength: how far each pixel is carried from its own color toward its blurred
/// neighbourhood. 0 is a no-op and 1 replaces the pixel outright, so the whole useful range is
/// on the slider and a light touch stays available — the brush is meant to be built up in
/// passes, the way a soft round is.
pub const BLUR_STRENGTH_MIN: f32 = 0.0;
pub const BLUR_STRENGTH_MAX: f32 = 1.0;
pub const BLUR_STRENGTH_DEFAULT: f32 = 0.5;
/// Kernel radius as a fraction of the brush radius. See `blur::blur_radius` for why the smear
/// deliberately does not reach the brush's own edge.
pub const BLUR_RADIUS_RATIO: f32 = 0.5;
/// Box-blur passes stacked to approximate a Gaussian. Three, not two: the window slides, so a
/// pass costs one add and one subtract per pixel *regardless of radius* — the price of the
/// third pass is a flat +50% on an already-linear step, not the radius-squared it would be
/// with a real Gaussian kernel, and two passes still read as a visible triangle.
pub const BLUR_BOX_PASSES: u32 = 3;

/// How sharp the eraser's rim is: 1 is the complete, hard-edged erase Calumma has always
/// had, 0 feathers all the way to the centre. Its own knob rather than the pen's brush,
/// because grain and flow have no meaning for taking ink away — only the edge does. Default 1
/// keeps every existing file and every existing habit intact.
pub const ERASER_HARDNESS_MIN: f32 = 0.0;
pub const ERASER_HARDNESS_MAX: f32 = 1.0;
pub const ERASER_HARDNESS_DEFAULT: f32 = 1.0;

pub const INK_OPACITY_MIN: f32 = 0.0;
pub const INK_OPACITY_MAX: f32 = 1.0;
pub const INK_OPACITY_DEFAULT: f32 = 1.0;

pub const FIT_PADDING: f32 = 0.99;
/// How far the camera may drift from a fit and still read as fitted: a thousandth of the
/// zoom, and a pixel of pan. The Fit control lights up while this holds, so the tolerance
/// exists to absorb float round-trips through `zoom_unit`, not to be generous — a real
/// nudge of the board has to switch it off.
pub const FIT_MATCH_ZOOM_TOLERANCE: f32 = 1e-3;
pub const FIT_MATCH_PAN_TOLERANCE: f32 = 1.0;
pub const ZOOM_STEP: f32 = 1.25;
/// How much of the viewport the paper still fills at the zoom floor. A fifth, so there is
/// real desk around a fitted board to drag it against and to see a large composition whole.
pub const MIN_ZOOM_FILL: f32 = 0.2;
/// The zoom ceiling, expressed as what it is for: the smallest span of document, in document
/// pixels, that may fill the shorter side of the viewport. Sixteen puts a single pixel under
/// a fingertip on any normal viewport, which is the point of zooming in this far — below that
/// `MAX_ZOOM_HARD` takes over as a flat cap. It is the *only* thing deriving the ceiling: the
/// floor and the ceiling are set from what each is for and share no constant, so moving one
/// cannot silently move the other.
pub const MIN_VISIBLE_DOC_SIDE: f32 = 16.0;
/// Above this zoom the board magnifies tiles with nearest-neighbour instead of bilinear, so
/// deep zoom shows pixels rather than a smooth gradient of them. Minification keeps its
/// filtering and its mip chain — that end is a downsample and wants both.
pub const CRISP_PIXEL_ZOOM: f32 = 4.0;
/// How much of the paper (as a fraction of whichever is smaller, paper or viewport)
/// has to stay on screen. Panning is free inside that slack, so the paper can be
/// dragged around even when it fits the viewport whole.
pub const PAN_KEEP_VISIBLE: f32 = 0.5;
/// Ceiling on the scroll-pan speed-up applied as the board zooms out. A drag moves the
/// board one-for-one with the pointer, but a scroll notch is a fixed number of pixels,
/// so without this a zoomed-out board crawls. Gain is `fit_zoom / zoom`, clamped to
/// `[1, SCROLL_PAN_MAX_GAIN]` — never below 1, so zooming in cannot slow scrolling down.
pub const SCROLL_PAN_MAX_GAIN: f32 = 4.0;
/// A trackpad reports precise per-pixel scroll deltas; a wheel reports lines — one or three
/// per notch. Lines are scaled to pixels by this so a notch moves a notch's worth of board
/// instead of a few pixels.
pub const SCROLL_LINE_PIXELS: f32 = 24.0;
/// Scroll-wheel zoom is `e^(delta * weight)`. A trackpad gesture is many small deltas and a
/// wheel notch is one big one, so the two units need different weights to land on a
/// comparable amount of zoom per gesture.
pub const ZOOM_PER_SCROLL_PIXEL: f32 = 0.01;
pub const ZOOM_PER_SCROLL_LINE: f32 = 0.08;
pub const MAX_ZOOM_HARD: f32 = 64.0;
pub const VIEWPORT_CULL_PADDING_PX: f32 = 1.0;

pub const AUTOSAVE_INTERVAL_MS: u64 = 800;
/// How many autosave ticks may pass on a busy engine before the save stops yielding to the
/// render loop and takes the lock outright. At `AUTOSAVE_INTERVAL_MS` a tick, this bounds the
/// worst case to a few seconds of unsaved work while keeping the common case — a save landing
/// mid-gesture — off the frame.
pub const AUTOSAVE_MAX_SKIPPED_TICKS: u32 = 4;
pub const RECENT_PROJECTS_LIMIT: usize = 32;
pub const WORKSPACES_LIMIT: usize = 64;
pub const PROJECT_THUMB_MAX_SIDE: u32 = 1024;

pub const IMPORT_MAX_SIDE: u32 = 4096;

pub const STROKE_INSTANCE_CAPACITY: usize = 1024;
pub const VECTOR_SHAPE_INSTANCE_CAPACITY: usize = 256;
/// Initial size of the per-frame tile instance buffer (origin + atlas slot per visible tile).
/// Unrelated to the atlas's own capacity: this just needs to hold one record per tile drawn
/// this frame, and grows the same way the stroke/vector-shape instance buffers do.
pub const TILE_INSTANCE_CAPACITY: usize = 1024;
/// Swapchain queue depth, fixed for the life of the surface. One, not two: this is an
/// interactive editor, so a shallower queue is always the right trade — a deeper one only buys
/// throughput headroom the board does not need, and costs a frame of pen-to-pixel latency.
///
/// It used to be raised and lowered around each camera gesture, which meant calling
/// `Surface::configure` mid-gesture — and wgpu drains the entire GPU queue before it will
/// reconfigure a surface (`Device::configure_surface` polls with `PollType::wait_indefinitely`).
/// That put a full pipeline stall on the main thread inside the first `mouseDragged` of every
/// pan, and another one four idle frames after the last. Whatever the deeper queue was worth,
/// it did not cover that.
pub const SURFACE_FRAME_LATENCY: u32 = 1;
pub const CAMERA_MOTION_IDLE_FRAMES: u32 = 4;
pub const GPU_TILE_RETENTION_MARGIN_TILES: i32 = 3;

/// Long-side cap on the cached per-layer preview the layers panel draws its thumbnails from.
/// The preview is kept in memory beside the tiles and rebuilt only when the layer's pixels
/// change, so every size the shell asks for — a 40pt row thumb, a hover card — is a cheap
/// resample of this instead of another full-resolution scan of the whole layer. 512 costs at
/// most 1 MiB per layer and is still several times any thumbnail the chrome shows, so it
/// survives future panel sizes without being rebuilt at a new resolution.
pub const LAYER_PREVIEW_MAX_SIDE: u32 = 512;

pub const OVERVIEW_MAX_SIDE: u32 = 2048;
pub const OVERVIEW_ENTER_TILE_THRESHOLD: usize = 48;
pub const OVERVIEW_EXIT_TILE_THRESHOLD: usize = 24;

/// Starting depth of the GPU tile atlas (one shared `texture_2d_array` every layer's tiles are
/// packed into, so a whole document layer draws in one instanced call instead of one draw per
/// tile). A `wgpu::Texture` reserves VRAM for its full declared layer count up front, so this
/// stays small — enough for a single-monitor viewport across a few layers without ever growing
/// — and the atlas doubles on demand from here up to `TILE_ATLAS_MAX_CAPACITY`. This is what
/// keeps the feature cheap on a low-end GPU: a small document never pays for a big array.
pub const TILE_ATLAS_INITIAL_CAPACITY: u32 = 128;
/// Hard ceiling on the tile atlas, independent of how large a `max_texture_array_layers` the
/// adapter reports. Bounds worst-case VRAM (`TILE_ATLAS_MAX_CAPACITY * TILE_BYTES`, here 1GiB)
/// for a pathological case — many fully-painted layers, zoomed out on a very large document —
/// so panning/zooming stays responsive by evicting the least-important tiles instead of
/// growing the allocation without limit.
pub const TILE_ATLAS_MAX_CAPACITY: u32 = 4096;

pub const ALPHA_OPAQUE: u8 = u8::MAX;
pub const ALPHA_MAX: u32 = u8::MAX as u32;
pub const ALPHA_ROUND_BIAS: u32 = ALPHA_MAX / 2;

pub const DEFAULT_INK: [u8; 4] = [26, 26, 26, ALPHA_OPAQUE];
/// The color Paper is created with. One constant, because both project creation and canvas
/// growth fill with it and a mismatch would show as a seam.
pub const PAPER_WHITE: [u8; 4] = [255, 255, 255, ALPHA_OPAQUE];

/// The eyedropper's sample area, as a radius in document pixels around the clicked pixel —
/// pixels whose centre is within `radius + 0.5` of the clicked centre are averaged, so 0 is
/// a single pixel and the default 1 is the 3×3 disc every image editor offers. Averaging is
/// the useful default because a single pixel off an antialiased edge or a grainy brush is
/// almost never the color the eye reads there.
pub const EYEDROPPER_RADIUS_MIN: u32 = 0;
pub const EYEDROPPER_RADIUS_MAX: u32 = 15;
pub const EYEDROPPER_RADIUS_DEFAULT: u32 = 1;

/// How close a neighbouring pixel has to be to the one clicked for the flood to keep going.
/// Compared as squared Euclidean distance over all four channels — see `fill::flood_region`.
/// Shared by the bucket and the magic wand: they are one traversal, so they are one tolerance.
pub const TOLERANCE_MIN: u8 = 0;
pub const TOLERANCE_MAX: u8 = 128;
pub const TOLERANCE_DEFAULT: u8 = 24;

/// One press of a Filters-menu Increase / Decrease item. Menus are discrete and
/// adjustments are continuous, so the step is a product constant and lives here — not
/// in the shell, which only names the menu item.
pub const ADJUSTMENT_NUDGE_STEP: f32 = 0.05;
/// Gamma is a multiplier around 1.0 on a 0.1–4.0 range, not a −1..1 offset, so it needs
/// a coarser step than the other four to move a visible amount per press.
pub const GAMMA_NUDGE_STEP: f32 = 0.1;

pub const MIN_SCALE: f32 = 0.02;
pub const MAX_SCALE: f32 = 50.0;

/// How far outside a vector item's own edge a click still picks it, in *screen* pixels so a
/// hairline stroke stays grabbable at every zoom. Converted to document units by the camera
/// in `Document::vector_item_at`, never by the shell.
pub const VECTOR_PICK_SLACK_PX: f32 = 6.0;
/// One arrow-key nudge of the selected vector item, in document pixels.
pub const VECTOR_NUDGE_STEP: f32 = 1.0;
/// Same step for a Move-tool / transform-mode layer offset nudge.
pub const LAYER_NUDGE_STEP: f32 = VECTOR_NUDGE_STEP;

pub const LOSSY_EXPORT_QUALITY: f32 = 0.92;

pub const MIN_CANVAS_SIDE: u32 = 16;
pub const MAX_CANVAS_SIDE: u32 = 8192;

/// Ruler tick spacing floor, in *screen* pixels — the doc-pixel step is chosen so a minor
/// tick never lands closer together than this, and a labeled major tick never closer than
/// `RULER_MIN_MAJOR_SPACING_PX`, at any zoom.
pub const RULER_MIN_MINOR_SPACING_PX: f32 = 8.0;
pub const RULER_MIN_MAJOR_SPACING_PX: f32 = 56.0;

/// How close, in *screen* pixels, a dragged edge has to come to a guide before it lands on
/// it. Screen-space so the snap feels the same at every zoom, exactly like the corner-handle
/// hit radius — converted to document units by the camera in `guide.rs`, never by the shell.
pub const GUIDE_SNAP_PX: f32 = 6.0;
/// How close a click has to be to a guide to grab it, in screen pixels. Wider than the line
/// itself so a one-pixel rule stays catchable.
pub const GUIDE_PICK_SLACK_PX: f32 = 5.0;
/// Two guides closer together than this in document pixels are the same guide, so dropping
/// one on top of another leaves one rather than a stack nothing can pull apart again.
pub const GUIDE_MIN_SEPARATION: f32 = 0.5;
/// Ceiling on guides per document. Guides are chrome, not content — a board that wants more
/// rules than this wants a grid.
pub const GUIDES_LIMIT: usize = 128;
