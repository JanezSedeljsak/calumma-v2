//! `SeamCarveOp` — the content-aware-resize Smart Tool. The carving itself is
//! `calumma_core::smarttools::seam_carving`'s job (see `engine/core/tests/smarttools/seam_carving.rs`); what belongs
//! here is the `Op` contract: it needs an explicit target size, refuses one outside the canvas
//! limits, and lands as a new layer through the shared `OpOutput::Raster` path.

use calumma_core::limits::{MAX_CANVAS_SIDE, MIN_CANVAS_SIDE};
use calumma_core::{Document, LayerContent};
use calumma_ops::{
    run_op_on_document, Backend, Op, OpError, OpInput, OpKind, OpOutput, OpParams, OpRegistry,
    SeamCarveOp,
};

fn raster(w: u32, h: u32) -> OpInput {
    OpInput::Raster {
        rgba: vec![120u8; (w as usize) * (h as usize) * 4],
        w,
        h,
    }
}

fn sized(w: u32, h: u32) -> OpParams {
    OpParams {
        target_size: Some((w, h)),
        ..Default::default()
    }
}

#[test]
fn is_a_core_op_always_available() {
    let op = SeamCarveOp;
    assert_eq!(op.kind(), OpKind::SeamCarve);
    assert_eq!(op.backend(), Backend::Core);
    assert!(op.available());
}

#[test]
fn carves_to_the_exact_target_size() {
    let op = SeamCarveOp;
    let out = op.run(raster(40, 40), &sized(32, 36)).unwrap();
    let OpOutput::Raster { w, h, rgba } = out else {
        panic!("expected a raster");
    };
    assert_eq!((w, h), (32, 36));
    assert_eq!(rgba.len(), (32 * 36 * 4) as usize);
}

#[test]
fn expanding_is_supported_too_not_just_shrinking() {
    let op = SeamCarveOp;
    let out = op.run(raster(20, 20), &sized(26, 20)).unwrap();
    let OpOutput::Raster { w, h, .. } = out else {
        panic!("expected a raster");
    };
    assert_eq!((w, h), (26, 20));
}

#[test]
fn a_missing_target_size_is_refused() {
    let op = SeamCarveOp;
    assert_eq!(
        op.run(raster(20, 20), &OpParams::default()),
        Err(OpError::BadInput)
    );
}

#[test]
fn a_target_outside_the_canvas_limits_is_refused() {
    let op = SeamCarveOp;
    for (w, h) in [
        (MIN_CANVAS_SIDE - 1, 20),
        (20, MIN_CANVAS_SIDE - 1),
        (MAX_CANVAS_SIDE + 1, 20),
        (20, MAX_CANVAS_SIDE + 1),
    ] {
        assert_eq!(
            op.run(raster(20, 20), &sized(w, h)),
            Err(OpError::BadInput),
            "{w}x{h} should be refused"
        );
    }
}

#[test]
fn non_raster_input_is_refused() {
    let op = SeamCarveOp;
    assert_eq!(
        op.run(OpInput::None, &sized(20, 20)),
        Err(OpError::BadInput)
    );
}

#[test]
fn running_it_on_a_document_adds_a_new_carved_layer() {
    let mut registry = OpRegistry::new();
    registry.register_core(Box::new(SeamCarveOp));
    let mut doc = Document::new("p".into(), "t", 32, 32);
    doc.add_layer("Source");
    let index = doc.layers.len() - 1;
    doc.layers[index]
        .tiles_mut()
        .unwrap()
        .set_pixel(4, 4, [200, 30, 30, 255]);
    let before = doc.layers.len();

    run_op_on_document(
        &registry,
        &mut doc,
        index,
        OpKind::SeamCarve,
        &sized(24, 32),
    )
    .unwrap();

    assert_eq!(doc.layers.len(), before + 1);
    let added = doc.layers.last().unwrap();
    assert!(matches!(added.content, LayerContent::Raster(_)));
    assert_eq!(added.tiles().unwrap().width(), 24);
    assert_eq!(added.tiles().unwrap().height(), 32);
}
