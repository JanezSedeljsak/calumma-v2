use calumma_ops::{Backend, Op, OpError, OpInput, OpKind, OpOutput, OpParams};
use std::os::raw::c_int;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;

#[repr(u32)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CalmOpKind {
    RemoveBackground = 0,
    GenerateTexture = 1,
    Vectorize = 2,
    SuggestShape = 3,
}

impl CalmOpKind {
    fn from_u32(v: u32) -> Option<Self> {
        match v {
            0 => Some(Self::RemoveBackground),
            1 => Some(Self::GenerateTexture),
            2 => Some(Self::Vectorize),
            3 => Some(Self::SuggestShape),
            _ => None,
        }
    }

    fn to_op_kind(self) -> OpKind {
        match self {
            Self::RemoveBackground => OpKind::RemoveBackground,
            Self::GenerateTexture => OpKind::GenerateTexture,
            Self::Vectorize => OpKind::Vectorize,
            Self::SuggestShape => OpKind::SuggestShape,
        }
    }
}

#[repr(u32)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CalmOpOutputKind {
    None = 0,
    Mask = 1,
    Raster = 2,
}

#[repr(C)]
pub struct CalmOpInput {
    pub rgba: *const u8,
    pub w: u32,
    pub h: u32,
}

#[repr(C)]
pub struct CalmOpOutput {
    pub kind: CalmOpOutputKind,
    pub data: *mut u8,
    pub len: usize,
    pub w: u32,
    pub h: u32,
}

pub type CalmOpAvailableFn = Option<unsafe extern "C" fn(CalmOpKind) -> bool>;
pub type CalmOpRunFn =
    Option<unsafe extern "C" fn(CalmOpKind, *const CalmOpInput, *mut CalmOpOutput) -> c_int>;
pub type CalmOpFreeFn = Option<unsafe extern "C" fn(*mut CalmOpOutput)>;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct CalmPlatformOps {
    pub available: CalmOpAvailableFn,
    pub run: CalmOpRunFn,
    pub free_output: CalmOpFreeFn,
}

pub struct PlatformOp {
    kind: OpKind,
    ops: CalmPlatformOps,
}

impl PlatformOp {
    pub fn for_kind(kind: OpKind, ops: CalmPlatformOps) -> Self {
        Self { kind, ops }
    }

    pub fn remove_background(ops: CalmPlatformOps) -> Self {
        Self::for_kind(OpKind::RemoveBackground, ops)
    }
}

impl Op for PlatformOp {
    fn kind(&self) -> OpKind {
        self.kind
    }

    fn backend(&self) -> Backend {
        Backend::Platform
    }

    fn available(&self) -> bool {
        let Some(available) = self.ops.available else {
            return false;
        };
        let kind = match self.kind {
            OpKind::RemoveBackground => CalmOpKind::RemoveBackground,
            OpKind::GenerateTexture => CalmOpKind::GenerateTexture,
            OpKind::Vectorize => CalmOpKind::Vectorize,
            OpKind::SuggestShape => CalmOpKind::SuggestShape,
        };
        catch_unwind(AssertUnwindSafe(|| unsafe { available(kind) })).unwrap_or(false)
    }

    fn run(&self, input: OpInput, _params: &OpParams) -> Result<OpOutput, OpError> {
        let Some(run) = self.ops.run else {
            return Err(OpError::Unavailable);
        };
        let kind = match self.kind {
            OpKind::RemoveBackground => CalmOpKind::RemoveBackground,
            OpKind::GenerateTexture => CalmOpKind::GenerateTexture,
            OpKind::Vectorize => CalmOpKind::Vectorize,
            OpKind::SuggestShape => CalmOpKind::SuggestShape,
        };
        let (rgba, w, h) = match input {
            OpInput::Raster { rgba, w, h } => (rgba, w, h),
            _ => return Err(OpError::BadInput),
        };
        let c_input = CalmOpInput {
            rgba: rgba.as_ptr(),
            w,
            h,
        };
        let mut out = CalmOpOutput {
            kind: CalmOpOutputKind::None,
            data: ptr::null_mut(),
            len: 0,
            w: 0,
            h: 0,
        };
        let status = catch_unwind(AssertUnwindSafe(|| unsafe {
            run(kind, &c_input, &mut out)
        }))
        .unwrap_or(1);
        if status != 0 {
            free_platform_output(&self.ops, &mut out);
            return Err(OpError::Failed(
                calumma_core::names::ERR_PLATFORM_OP_FAILED.into(),
            ));
        }
        let result = match out.kind {
            CalmOpOutputKind::Mask => {
                if out.data.is_null() || out.len == 0 {
                    Err(OpError::Failed(calumma_core::names::ERR_EMPTY_MASK.into()))
                } else {
                    let slice = unsafe { std::slice::from_raw_parts(out.data, out.len) };
                    Ok(OpOutput::Mask(slice.to_vec()))
                }
            }
            CalmOpOutputKind::Raster => {
                if out.data.is_null() || out.len == 0 || out.w == 0 || out.h == 0 {
                    Err(OpError::Failed(
                        calumma_core::names::ERR_EMPTY_RASTER.into(),
                    ))
                } else {
                    let slice = unsafe { std::slice::from_raw_parts(out.data, out.len) };
                    Ok(OpOutput::Raster {
                        rgba: slice.to_vec(),
                        w: out.w,
                        h: out.h,
                    })
                }
            }
            CalmOpOutputKind::None => {
                Err(OpError::Failed(calumma_core::names::ERR_NO_OUTPUT.into()))
            }
        };
        free_platform_output(&self.ops, &mut out);
        result
    }
}

fn free_platform_output(ops: &CalmPlatformOps, out: &mut CalmOpOutput) {
    if let Some(free_output) = ops.free_output {
        let _ = catch_unwind(AssertUnwindSafe(|| unsafe {
            free_output(out);
        }));
    }
}

pub fn parse_op_kind(v: u32) -> Option<OpKind> {
    CalmOpKind::from_u32(v).map(|k| k.to_op_kind())
}
