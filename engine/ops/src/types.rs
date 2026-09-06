use calumma_core::VectorPath;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OpKind {
    RemoveBackground,
    GenerateTexture,
    Vectorize,
    SuggestShape,
    /// Deterministic Lanczos-3 resampling — `engine/core/src/smarttools/resample.rs`. Always
    /// available, `Backend::Core`, no trained weights.
    Upscale,
    /// Seam-carving content-aware resize — `engine/core/src/smarttools/seam_carving.rs`. Always
    /// available, `Backend::Core`.
    SeamCarve,
    /// Deterministic graph-cut background matting — `engine/core/src/smarttools/grabcut.rs`.
    /// `Backend::Core`, a from-scratch alternative to the `RemoveBackground` Vision op rather
    /// than a replacement for it — the two are separate `OpKind`s so both can show up as their
    /// own Smart Tool.
    SmartMatte,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    Core,
    Platform,
}

#[derive(Clone, Debug, PartialEq)]
pub enum OpInput {
    Raster { rgba: Vec<u8>, w: u32, h: u32 },
    Prompt(String),
    None,
}

#[derive(Clone, Debug, PartialEq)]
pub enum OpOutput {
    Mask(Vec<u8>),
    Raster { rgba: Vec<u8>, w: u32, h: u32 },
    Paths(Vec<VectorPath>),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct OpParams {
    pub prompt: Option<String>,
    /// `Upscale`: the layer's current side lengths multiplied by this factor. `None` defaults
    /// to 2×.
    pub scale: Option<f32>,
    /// `SeamCarve`: the exact target size to carve or expand toward.
    pub target_size: Option<(u32, u32)>,
    /// `SmartMatte`: the region the user drew around the subject, one byte per document pixel,
    /// non-zero inside. `None` falls back to automatic seeding. This is what turns the matte
    /// from a guess about where the subject probably is into a cut against a boundary someone
    /// actually asserted — see `calumma_core::smarttools::grabcut::foreground_matte_in_region`.
    pub seed_region: Option<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpError {
    Unavailable,
    Failed(String),
    BadInput,
    BadLayer,
}

impl fmt::Display for OpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable => write!(f, "{}", calumma_core::names::ERR_OP_UNAVAILABLE),
            Self::Failed(msg) => write!(f, "{}{msg}", calumma_core::names::ERR_OP_FAILED_PREFIX),
            Self::BadInput => write!(f, "{}", calumma_core::names::ERR_BAD_INPUT),
            Self::BadLayer => write!(f, "{}", calumma_core::names::ERR_BAD_LAYER),
        }
    }
}

impl std::error::Error for OpError {}

pub trait Op: Send + Sync {
    fn kind(&self) -> OpKind;
    fn backend(&self) -> Backend;
    fn available(&self) -> bool;
    fn run(&self, input: OpInput, params: &OpParams) -> Result<OpOutput, OpError>;
}
