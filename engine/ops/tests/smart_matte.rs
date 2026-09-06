//! `SmartMatteOp` — graph-cut background removal as a Smart Tool. The segmentation itself is
//! covered in `engine/core/tests/smarttools/grabcut.rs`; this pins the `Op` contract and, most
//! importantly, that its output travels the existing mask path into the document — the same
//! one Vision's Remove Background already uses, so it inherits the same undo step.

use calumma_core::{Document, Layer};
use calumma_ops::{
    run_op_on_document, Backend, Op, OpError, OpInput, OpKind, OpOutput, OpParams, OpRegistry,
    SmartMatteOp,
};

/// A red square on a blue field, as an `OpInput`.
fn blob(w: u32, h: u32) -> OpInput {
    let mut rgba = [20u8, 40, 200, 255].repeat((w * h) as usize);
    for y in h / 4..h * 3 / 4 {
        for x in w / 4..w * 3 / 4 {
            let i = ((y * w + x) * 4) as usize;
            rgba[i..i + 4].copy_from_slice(&[220, 30, 30, 255]);
        }
    }
    OpInput::Raster { rgba, w, h }
}

#[test]
fn is_a_core_op_always_available() {
    let op = SmartMatteOp;
    assert_eq!(op.kind(), OpKind::SmartMatte);
    assert_eq!(op.backend(), Backend::Core);
    assert!(op.available());
}

/// Its own kind, deliberately — sharing `RemoveBackground` would put it behind Vision in the
/// registry's platform-beats-core resolution and it would never run.
#[test]
fn it_does_not_collide_with_the_vision_remove_background_op() {
    assert_ne!(SmartMatteOp.kind(), OpKind::RemoveBackground);
}

#[test]
fn it_returns_a_mask_the_size_of_its_input() {
    let op = SmartMatteOp;
    let out = op.run(blob(48, 48), &OpParams::default()).unwrap();
    let OpOutput::Mask(mask) = out else {
        panic!("expected a mask");
    };
    assert_eq!(mask.len(), 48 * 48);
    assert_eq!(mask[24 * 48 + 24], 255, "the blob is kept");
    assert_eq!(mask[2 * 48 + 2], 0, "the field is cut");
}

#[test]
fn an_empty_layer_is_a_refusal_not_an_all_zero_mask() {
    let op = SmartMatteOp;
    let empty = OpInput::Raster {
        rgba: vec![0u8; 32 * 32 * 4],
        w: 32,
        h: 32,
    };
    assert!(matches!(
        op.run(empty, &OpParams::default()),
        Err(OpError::Failed(_))
    ));
}

#[test]
fn non_raster_input_is_refused() {
    let op = SmartMatteOp;
    assert_eq!(
        op.run(OpInput::None, &OpParams::default()),
        Err(OpError::BadInput)
    );
}

/// The payoff of reusing `OpOutput::Mask`: it lands through `apply_remove_background_mask`,
/// which is already undoable, so the tool gets ⌘Z for free rather than inventing a history step.
#[test]
fn running_it_on_a_document_bakes_the_matte_and_is_undoable() {
    let mut registry = OpRegistry::new();
    registry.register_core(Box::new(SmartMatteOp));
    let (w, h) = (48u32, 48u32);
    let mut doc = Document::new("p".into(), "t", w, h);
    doc.layers.push(Layer::new("Subject", w, h));
    let index = doc.layers.len() - 1;
    {
        let tiles = doc.layers[index].tiles_mut().unwrap();
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                let inside = (12..36).contains(&x) && (12..36).contains(&y);
                let px = if inside {
                    [220, 30, 30, 255]
                } else {
                    [20, 40, 200, 255]
                };
                tiles.set_pixel(x, y, px);
            }
        }
    }

    run_op_on_document(
        &registry,
        &mut doc,
        index,
        OpKind::SmartMatte,
        &OpParams::default(),
    )
    .unwrap();

    let after = doc.layers[index].tiles().unwrap().get_pixel(2, 2);
    assert_eq!(after[3], 0, "the background corner was matted away");
    let kept = doc.layers[index].tiles().unwrap().get_pixel(24, 24);
    assert_eq!(kept[3], 255, "the subject is untouched");

    assert!(doc.undo(), "the mask bake left an undo step behind");
    let restored = doc.layers[index].tiles().unwrap().get_pixel(2, 2);
    assert_eq!(restored[3], 255, "undo brought the background back");
}

/// Two identical subjects, a region drawn around only one of them: the param has to reach the
/// segmentation, or both would survive. This is the plumbing test for `OpParams::seed_region` —
/// what the region *means* is core's business (`engine/core/tests/smarttools/grabcut.rs`).
#[test]
fn a_seed_region_reaches_the_segmentation_and_changes_the_result() {
    let (w, h) = (64u32, 64u32);
    let mut rgba = [20u8, 40, 200, 255].repeat((w * h) as usize);
    for y in 24..40u32 {
        for x in 8..24u32 {
            let i = ((y * w + x) * 4) as usize;
            rgba[i..i + 4].copy_from_slice(&[220, 30, 30, 255]);
        }
        for x in 40..56u32 {
            let i = ((y * w + x) * 4) as usize;
            rgba[i..i + 4].copy_from_slice(&[220, 30, 30, 255]);
        }
    }
    // A box around the left subject only.
    let mut region = vec![0u8; (w * h) as usize];
    for y in 18..46u32 {
        for x in 2..30u32 {
            region[(y * w + x) as usize] = 255;
        }
    }

    let op = SmartMatteOp;
    let with_region = OpParams {
        seed_region: Some(region),
        ..Default::default()
    };
    let OpOutput::Mask(drawn) = op
        .run(
            OpInput::Raster {
                rgba: rgba.clone(),
                w,
                h,
            },
            &with_region,
        )
        .unwrap()
    else {
        panic!("expected a mask");
    };

    assert_eq!(
        drawn[(32 * w + 16) as usize],
        255,
        "the encircled subject is kept"
    );
    assert_eq!(
        drawn[(32 * w + 48) as usize],
        0,
        "the identical one outside the region is cut — so the region was honoured"
    );
}
