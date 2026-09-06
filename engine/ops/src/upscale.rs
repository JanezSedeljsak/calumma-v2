//! The Upscale Smart Tool: `OpKind::Upscale`, always available, `Backend::Core` — Lanczos-3 is
//! deterministic math (`calumma_core::smarttools::resample`), not a model, so there is no `available()`
//! gate and no platform counterpart to lose to.

use crate::types::{Backend, Op, OpError, OpInput, OpKind, OpOutput, OpParams};
use calumma_core::limits::MAX_CANVAS_SIDE;
use calumma_core::smarttools::resample::lanczos3_resize;

pub struct UpscaleOp;

impl Op for UpscaleOp {
    fn kind(&self) -> OpKind {
        OpKind::Upscale
    }

    fn backend(&self) -> Backend {
        Backend::Core
    }

    fn available(&self) -> bool {
        true
    }

    fn run(&self, input: OpInput, params: &OpParams) -> Result<OpOutput, OpError> {
        let OpInput::Raster { rgba, w, h } = input else {
            return Err(OpError::BadInput);
        };
        if w == 0 || h == 0 || rgba.len() != (w as usize) * (h as usize) * 4 {
            return Err(OpError::BadInput);
        }
        let scale = params.scale.unwrap_or(2.0);
        // A scale at or below 1 is not what this tool is for — `resample` handles shrinking
        // correctly, but "Upscale" asking to shrink is almost certainly a caller bug, not a
        // deliberate downscale, so it is refused rather than silently honoured.
        if !scale.is_finite() || scale <= 1.0 {
            return Err(OpError::BadInput);
        }
        let dst_w = ((w as f32) * scale)
            .round()
            .clamp(1.0, MAX_CANVAS_SIDE as f32) as u32;
        let dst_h = ((h as f32) * scale)
            .round()
            .clamp(1.0, MAX_CANVAS_SIDE as f32) as u32;
        let rgba = lanczos3_resize(&rgba, w, h, dst_w, dst_h);
        Ok(OpOutput::Raster {
            rgba,
            w: dst_w,
            h: dst_h,
        })
    }
}
