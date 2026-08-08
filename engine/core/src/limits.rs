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
pub const MAX_ZOOM_HARD: f32 = 64.0;
pub const VIEWPORT_CULL_PADDING_PX: f32 = 1.0;

pub const AUTOSAVE_INTERVAL_MS: u64 = 800;
pub const RECENT_PROJECTS_LIMIT: usize = 32;

pub const IMPORT_MAX_SIDE: u32 = 4096;

pub const STROKE_INSTANCE_CAPACITY: usize = 1024;
pub const SURFACE_FRAME_LATENCY: u32 = 2;
pub const GPU_TILE_RETENTION_MARGIN_TILES: i32 = 1;

pub const ALPHA_OPAQUE: u8 = u8::MAX;
pub const ALPHA_MAX: u32 = u8::MAX as u32;
pub const ALPHA_ROUND_BIAS: u32 = ALPHA_MAX / 2;

pub const DEFAULT_INK: [u8; 4] = [26, 26, 26, ALPHA_OPAQUE];

pub const FILL_TOLERANCE_DEFAULT: u8 = 24;

pub const MIN_SCALE: f32 = 0.02;
pub const MAX_SCALE: f32 = 50.0;

pub const MIN_CANVAS_SIDE: u32 = 16;
pub const MAX_CANVAS_SIDE: u32 = 8192;
