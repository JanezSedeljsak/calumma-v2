//! The Smart Matte tool: `OpKind::SmartMatte` — GrabCut-style graph-cut
//! background removal (`calumma_core::smarttools::grabcut`), deterministic and dependency-free.

use crate::types::{Op, OpError, OpInput, OpKind, OpOutput, OpParams};
use calumma_core::smarttools::grabcut::{
    foreground_matte, foreground_matte_in_region, MatteParams,
};

pub struct SmartMatteOp;

impl Op for SmartMatteOp {
    fn kind(&self) -> OpKind {
        OpKind::SmartMatte
    }

    fn available(&self) -> bool {
        true
    }

    fn run(&self, input: OpInput, params: &OpParams) -> Result<OpOutput, OpError> {
        let OpInput { rgba, w, h } = input;
        if w == 0 || h == 0 || rgba.len() != (w as usize) * (h as usize) * 4 {
            return Err(OpError::BadInput);
        }
        let matte_params = MatteParams::default();
        // A drawn region, when there is one, is the far better seeding: it says outright which
        // side of the boundary the caller means, instead of the border ring's assumption that
        // the subject sits somewhere in the middle.
        let mask = match params.seed_region.as_deref() {
            Some(region) => foreground_matte_in_region(&rgba, w, h, region, &matte_params),
            None => foreground_matte(&rgba, w, h, &matte_params),
        }
        // No foreground to find is a refusal, not an all-zero mask: applying that would read as
        // "removed the whole layer," which is never what was asked for.
        .ok_or_else(|| OpError::Failed("no foreground found".into()))?;
        Ok(OpOutput::Mask(mask))
    }
}
