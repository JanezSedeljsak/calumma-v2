use calumma_core::VectorPath;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OpKind {
    RemoveBackground,
    GenerateTexture,
    Vectorize,
    SuggestShape,
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
