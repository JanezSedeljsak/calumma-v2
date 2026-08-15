pub const HISTORY_MEMORY_BUDGET_BYTES: usize = 256 * 1024 * 1024;

pub const STROKE_POINT_CAPACITY: usize = 256;
pub const MIN_STROKE_POINT_DISTANCE: f32 = 0.5;
pub const STAMP_SPACING_RATIO: f32 = 0.5;
pub const MIN_STAMP_SPACING: f32 = 0.5;
pub const STAMP_COVERAGE_PADDING: f32 = 1.0;

pub const BRUSH_SIZE_MIN: f32 = 1.0;
pub const BRUSH_SIZE_MAX: f32 = 96.0;
pub const BRUSH_SIZE_DEFAULT: f32 = 3.0;

pub const FIT_PADDING: f32 = 0.99;
pub const ZOOM_STEP: f32 = 1.25;
pub const MIN_ZOOM_FILL: f32 = 0.5;
pub const MAX_ZOOM_IN_FACTOR: f32 = 10.0;
pub const MIN_VISIBLE_DOC_SIDE: f32 = 400.0;
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
pub const RECENT_PROJECTS_LIMIT: usize = 32;
pub const WORKSPACES_LIMIT: usize = 64;
pub const PROJECT_THUMB_MAX_SIDE: u32 = 1024;

pub const IMPORT_MAX_SIDE: u32 = 4096;

pub const STROKE_INSTANCE_CAPACITY: usize = 1024;
pub const VECTOR_SHAPE_INSTANCE_CAPACITY: usize = 256;
pub const SURFACE_FRAME_LATENCY: u32 = 2;
pub const GPU_TILE_RETENTION_MARGIN_TILES: i32 = 1;

pub const ALPHA_OPAQUE: u8 = u8::MAX;
pub const ALPHA_MAX: u32 = u8::MAX as u32;
pub const ALPHA_ROUND_BIAS: u32 = ALPHA_MAX / 2;

pub const DEFAULT_INK: [u8; 4] = [26, 26, 26, ALPHA_OPAQUE];
/// The colour Paper is created with. One constant, because both project creation and canvas
/// growth fill with it and a mismatch would show as a seam.
pub const PAPER_WHITE: [u8; 4] = [255, 255, 255, ALPHA_OPAQUE];

pub const FILL_TOLERANCE_DEFAULT: u8 = 24;

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

pub const MIN_CANVAS_SIDE: u32 = 16;
pub const MAX_CANVAS_SIDE: u32 = 8192;
