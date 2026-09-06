//! The Seam Carve Smart Tool: `OpKind::SeamCarve`, always available, `Backend::Core` —
//! deterministic dynamic programming (`calumma_core::smarttools::seam_carving`), not a model.

use crate::types::{Backend, Op, OpError, OpInput, OpKind, OpOutput, OpParams};
use calumma_core::limits::{MAX_CANVAS_SIDE, MIN_CANVAS_SIDE};
use calumma_core::smarttools::seam_carving::seam_carve;

pub struct SeamCarveOp;

impl Op for SeamCarveOp {
    fn kind(&self) -> OpKind {
        OpKind::SeamCarve
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
        let Some((dst_w, dst_h)) = params.target_size else {
            return Err(OpError::BadInput);
        };
        if dst_w < MIN_CANVAS_SIDE
            || dst_h < MIN_CANVAS_SIDE
            || dst_w > MAX_CANVAS_SIDE
            || dst_h > MAX_CANVAS_SIDE
        {
            return Err(OpError::BadInput);
        }
        let rgba = seam_carve(&rgba, w, h, dst_w, dst_h);
        Ok(OpOutput::Raster {
            rgba,
            w: dst_w,
            h: dst_h,
        })
    }
}
