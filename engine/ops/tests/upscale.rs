//! `UpscaleOp` — the Lanczos-3 Smart Tool. Pure boundary tests: the math itself is
//! `calumma_core::smarttools::resample`'s job (see `engine/core/tests/smarttools/resample.rs`); what belongs here is
//! the `Op` contract — input/param validation, size math, and that it lands in the document as
//! a new layer via the same `OpOutput::Raster` path every other image-producing op uses.

use calumma_core::{Document, LayerContent};
use calumma_ops::{apply_output, run_op_on_document, Backend, Op, OpError, OpInput, OpParams};
use calumma_ops::{OpKind, OpRegistry, UpscaleOp};

fn raster(w: u32, h: u32) -> OpInput {
    OpInput::Raster {
        rgba: vec![10u8; (w as usize) * (h as usize) * 4],
        w,
        h,
    }
}

#[test]
fn is_a_core_op_always_available() {
    let op = UpscaleOp;
    assert_eq!(op.kind(), OpKind::Upscale);
    assert_eq!(op.backend(), Backend::Core);
    assert!(op.available());
}

#[test]
fn default_scale_doubles_both_sides() {
    let op = UpscaleOp;
    let out = op.run(raster(10, 20), &OpParams::default()).unwrap();
    let calumma_ops::OpOutput::Raster { w, h, rgba } = out else {
        panic!("expected a raster");
    };
    assert_eq!((w, h), (20, 40));
    assert_eq!(rgba.len(), (20 * 40 * 4) as usize);
}

#[test]
fn an_explicit_scale_is_honoured_and_rounded() {
    let op = UpscaleOp;
    let params = OpParams {
        scale: Some(1.5),
        ..Default::default()
    };
    let out = op.run(raster(10, 10), &params).unwrap();
    let calumma_ops::OpOutput::Raster { w, h, .. } = out else {
        panic!("expected a raster");
    };
    assert_eq!((w, h), (15, 15));
}

#[test]
fn a_scale_at_or_below_one_is_refused() {
    let op = UpscaleOp;
    for scale in [1.0, 0.5, 0.0, -2.0] {
        let params = OpParams {
            scale: Some(scale),
            ..Default::default()
        };
        assert_eq!(
            op.run(raster(4, 4), &params),
            Err(OpError::BadInput),
            "scale {scale} should be refused, not silently downscaled"
        );
    }
}

#[test]
fn a_non_finite_scale_is_refused_rather_than_panicking() {
    let op = UpscaleOp;
    for scale in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let params = OpParams {
            scale: Some(scale),
            ..Default::default()
        };
        assert_eq!(op.run(raster(4, 4), &params), Err(OpError::BadInput));
    }
}

#[test]
fn non_raster_input_is_refused() {
    let op = UpscaleOp;
    assert_eq!(
        op.run(OpInput::None, &OpParams::default()),
        Err(OpError::BadInput)
    );
    assert_eq!(
        op.run(OpInput::Prompt("x".into()), &OpParams::default()),
        Err(OpError::BadInput)
    );
}

#[test]
fn an_absurd_scale_clamps_to_the_canvas_ceiling_rather_than_allocating_forever() {
    let op = UpscaleOp;
    let params = OpParams {
        scale: Some(1e9),
        ..Default::default()
    };
    let out = op.run(raster(4, 4), &params).unwrap();
    let calumma_ops::OpOutput::Raster { w, h, .. } = out else {
        panic!("expected a raster");
    };
    assert_eq!(w, calumma_core::limits::MAX_CANVAS_SIDE);
    assert_eq!(h, calumma_core::limits::MAX_CANVAS_SIDE);
}

#[test]
fn running_it_on_a_document_adds_a_new_upscaled_layer() {
    let mut registry = OpRegistry::new();
    registry.register_core(Box::new(UpscaleOp));
    let mut doc = Document::new("p".into(), "t", 8, 8);
    doc.add_layer("Source");
    let index = doc.layers.len() - 1;
    doc.layers[index]
        .tiles_mut()
        .unwrap()
        .set_pixel(2, 2, [9, 8, 7, 255]);
    let before = doc.layers.len();

    run_op_on_document(
        &registry,
        &mut doc,
        index,
        OpKind::Upscale,
        &OpParams::default(),
    )
    .unwrap();

    assert_eq!(doc.layers.len(), before + 1);
    let added = doc.layers.last().unwrap();
    assert!(matches!(added.content, LayerContent::Raster(_)));
    assert_eq!(added.tiles().unwrap().width(), 16);
    assert_eq!(added.tiles().unwrap().height(), 16);
    assert_eq!(doc.active_layer, doc.layers.len() - 1);
}

#[test]
fn apply_output_rejects_a_raster_whose_byte_length_does_not_match_its_own_dimensions() {
    let mut doc = Document::new("p".into(), "t", 8, 8);
    let bad = calumma_ops::OpOutput::Raster {
        rgba: vec![0u8; 3],
        w: 4,
        h: 4,
    };
    assert_eq!(apply_output(&mut doc, 0, bad), Err(OpError::BadInput));
}
