use calumma_core::filters::{AdjustmentLut, Adjustments};
use calumma_core::tile::{TileCoord, TILE_BYTES, TILE_SIZE};
use calumma_core::{BrushProfile, Document, Layer, Selection, SelectionShape, StrokePoint, Tool};
use calumma_render::compose::*;

fn opaque_tile() -> Vec<u8> {
    vec![255u8; TILE_BYTES]
}

fn paint_layer() -> Layer {
    let mut doc = Document::new("t".to_string(), "Compose", 64, 64);
    doc.add_layer("Paint");
    doc.layers.pop().expect("layer")
}

#[test]
fn rgba_unit_maps_bytes_onto_the_zero_to_one_range() {
    assert_eq!(rgba_unit([0, 0, 0, 0]), [0.0, 0.0, 0.0, 0.0]);
    assert_eq!(rgba_unit([255, 255, 255, 255]), [1.0, 1.0, 1.0, 1.0]);
    let mid = rgba_unit([128, 64, 32, 16]);
    assert!((mid[0] - 128.0 / 255.0).abs() < 1e-6);
    assert!((mid[1] - 64.0 / 255.0).abs() < 1e-6);
    assert!((mid[2] - 32.0 / 255.0).abs() < 1e-6);
    assert!((mid[3] - 16.0 / 255.0).abs() < 1e-6);
}

#[test]
fn stroke_instances_is_empty_for_no_points() {
    assert!(stroke_instances(&[], 2.0, [1.0, 0.0, 0.0, 1.0], &BrushProfile::HARD).is_empty());
}

#[test]
fn a_single_point_becomes_one_degenerate_dot_segment() {
    let out = stroke_instances(
        &[StrokePoint { x: 3.0, y: 4.0 }],
        2.5,
        [1.0, 0.0, 0.0, 1.0],
        &BrushProfile::HARD,
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].segment, [3.0, 4.0, 3.0, 4.0]);
    assert_eq!(out[0].brush[0], 2.5);
}

#[test]
fn n_points_become_n_minus_one_segments_joined_end_to_end() {
    let points: Vec<StrokePoint> = (0..5)
        .map(|i| StrokePoint {
            x: i as f32,
            y: (i * 2) as f32,
        })
        .collect();
    let out = stroke_instances(&points, 1.0, [0.0, 1.0, 0.0, 1.0], &BrushProfile::HARD);
    assert_eq!(out.len(), 4);
    assert_eq!(out[0].segment, [0.0, 0.0, 1.0, 2.0]);
    assert_eq!(out[3].segment, [3.0, 6.0, 4.0, 8.0]);
    for pair in out.windows(2) {
        assert_eq!(
            [pair[0].segment[2], pair[0].segment[3]],
            [pair[1].segment[0], pair[1].segment[1]]
        );
    }
}

#[test]
fn layer_highlight_outline_marches() {
    let corners = [(0.0, 0.0), (100.0, 0.0), (100.0, 50.0), (0.0, 50.0)];
    let a = layer_highlight_instances(corners, 0.0, 1.0);
    let b = layer_highlight_instances(corners, 0.25, 1.0);
    assert!(!a.is_empty());
    assert!(a.iter().zip(b.iter()).any(|(x, y)| x.segment != y.segment));
}

/// The dash is chrome like the width is: zooming in four times has to cut four times as many
/// dashes out of the same edge, or the pattern grows with the board.
#[test]
fn layer_highlight_dashes_keep_a_constant_screen_period() {
    let corners = [(0.0, 0.0), (100.0, 0.0), (100.0, 50.0), (0.0, 50.0)];
    let at_one = layer_highlight_instances(corners, 0.0, 1.0).len();
    let at_four = layer_highlight_instances(corners, 0.0, 4.0).len();
    assert!(
        at_four >= at_one * 3,
        "expected ~4x the dashes at 4x zoom, got {at_four} against {at_one}"
    );
}

#[test]
fn transform_overlay_draws_four_edges_a_stem_and_five_handles() {
    let corners = [(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (0.0, 8.0)];
    let rotate = (5.0, -6.0);
    let out = transform_overlay_instances((0, corners, rotate));
    assert_eq!(out.len(), 10);

    for i in 0..4 {
        let a = corners[i];
        let b = corners[(i + 1) % 4];
        assert_eq!(out[i].segment, [a.0, a.1, b.0, b.1]);
    }
    assert_eq!(out[4].segment, [5.0, 0.0, rotate.0, rotate.1]);

    for handle in &out[5..] {
        assert_eq!(handle.segment[0], handle.segment[2]);
        assert_eq!(handle.segment[1], handle.segment[3]);
    }
    assert_eq!(out[9].segment, [rotate.0, rotate.1, rotate.0, rotate.1]);
}

#[test]
fn the_rotate_stem_starts_at_the_midpoint_of_the_top_edge() {
    let corners = [(2.0, 4.0), (12.0, 4.0), (12.0, 20.0), (2.0, 20.0)];
    let out = transform_overlay_instances((0, corners, (7.0, -2.0)));
    assert_eq!(out[4].segment[0], 7.0);
    assert_eq!(out[4].segment[1], 4.0);
}

#[test]
fn selection_helpers_return_none_without_a_selection() {
    let doc = Document::new("t".to_string(), "None", 32, 32);
    assert!(selection_rect_or_ellipse(&doc).is_none());
    assert!(selection_lasso_points(&doc).is_none());
}

#[test]
fn rect_and_ellipse_selections_map_to_their_preview_tool() {
    let mut doc = Document::new("t".to_string(), "Sel", 32, 32);

    doc.selection = Some(Selection {
        shape: SelectionShape::Rect {
            start: (1.0, 2.0),
            end: (9.0, 8.0),
        },
    });
    let (start, end, tool) = selection_rect_or_ellipse(&doc).expect("rect");
    assert_eq!(start, [1.0, 2.0]);
    assert_eq!(end, [9.0, 8.0]);
    assert_eq!(tool, Tool::Rect);
    assert!(selection_lasso_points(&doc).is_none());

    doc.selection = Some(Selection {
        shape: SelectionShape::Ellipse {
            start: (0.0, 0.0),
            end: (4.0, 4.0),
        },
    });
    assert_eq!(
        selection_rect_or_ellipse(&doc).expect("ellipse").2,
        Tool::Ellipse
    );
}

#[test]
fn a_lasso_selection_is_closed_back_to_its_first_point() {
    let mut doc = Document::new("t".to_string(), "Lasso", 32, 32);
    doc.selection = Some(Selection {
        shape: SelectionShape::Lasso {
            points: vec![(0.0, 0.0), (5.0, 0.0), (5.0, 5.0)],
        },
    });
    assert!(selection_rect_or_ellipse(&doc).is_none());
    let points = selection_lasso_points(&doc).expect("lasso");
    assert_eq!(points.len(), 4);
    assert_eq!((points[0].x, points[0].y), (0.0, 0.0));
    assert_eq!((points[3].x, points[3].y), (0.0, 0.0));
}

#[test]
fn an_empty_lasso_stays_empty_rather_than_closing_onto_nothing() {
    let mut doc = Document::new("t".to_string(), "Empty", 32, 32);
    doc.selection = Some(Selection {
        shape: SelectionShape::Lasso { points: Vec::new() },
    });
    assert!(selection_lasso_points(&doc).expect("lasso").is_empty());
}

#[test]
fn a_plain_layer_needs_no_reupload_payload() {
    let layer = paint_layer();
    assert!(
        composited_tile_payload(&opaque_tile(), TileCoord { x: 0, y: 0 }, &layer, None, 64)
            .is_none()
    );
}

#[test]
fn a_neutral_adjustment_lut_is_treated_as_no_adjustment_at_all() {
    let layer = paint_layer();
    let lut = AdjustmentLut::new(&Adjustments::default());
    assert!(composited_tile_payload(
        &opaque_tile(),
        TileCoord { x: 0, y: 0 },
        &layer,
        Some(&lut),
        64
    )
    .is_none());
}

#[test]
fn layer_opacity_scales_alpha_and_leaves_color_untouched() {
    let mut layer = paint_layer();
    layer.opacity = 0.5;
    let out = composited_tile_payload(&opaque_tile(), TileCoord { x: 0, y: 0 }, &layer, None, 64)
        .expect("payload");
    assert_eq!(out.len(), TILE_BYTES);
    assert_eq!(&out[0..3], &[255, 255, 255]);
    assert_eq!(out[3], 128);
}

#[test]
fn a_fully_transparent_layer_zeroes_every_alpha() {
    let mut layer = paint_layer();
    layer.opacity = 0.0;
    let out = composited_tile_payload(&opaque_tile(), TileCoord { x: 0, y: 0 }, &layer, None, 64)
        .expect("payload");
    assert!(out.chunks_exact(4).all(|px| px[3] == 0));
}

#[test]
fn a_mask_multiplies_alpha_per_pixel_without_touching_the_tile_bytes() {
    let doc_width = TILE_SIZE;
    let mut layer = paint_layer();
    let mut mask = vec![255u8; (doc_width * doc_width) as usize];
    mask[0] = 0;
    mask[1] = 128;
    layer.set_mask(Some(mask));

    let out = composited_tile_payload(
        &opaque_tile(),
        TileCoord { x: 0, y: 0 },
        &layer,
        None,
        doc_width,
    )
    .expect("payload");

    assert_eq!(out[3], 0);
    assert_eq!(out[7], 128);
    assert_eq!(out[11], 255);
    assert_eq!(&out[0..3], &[255, 255, 255]);
}

#[test]
fn mask_lookups_outside_the_document_are_skipped_rather_than_wrapping() {
    let doc_width = TILE_SIZE;
    let mut layer = paint_layer();
    layer.set_mask(Some(vec![0u8; (doc_width * doc_width) as usize]));

    let out = composited_tile_payload(
        &opaque_tile(),
        TileCoord { x: -1, y: -1 },
        &layer,
        None,
        doc_width,
    )
    .expect("payload");

    assert_eq!(out[3], 255);
}

#[test]
fn adjustments_rewrite_color_but_never_alpha() {
    let layer = paint_layer();
    let lut = AdjustmentLut::new(&Adjustments {
        brightness: -1.0,
        ..Default::default()
    });
    let out = composited_tile_payload(
        &opaque_tile(),
        TileCoord { x: 0, y: 0 },
        &layer,
        Some(&lut),
        64,
    )
    .expect("payload");
    assert_eq!(out[3], 255);
    assert!(out[0] < 255);
}

#[test]
fn a_short_input_tile_is_padded_to_a_full_tile() {
    let mut layer = paint_layer();
    layer.opacity = 0.5;
    let out = composited_tile_payload(&[255u8; 8], TileCoord { x: 0, y: 0 }, &layer, None, 64)
        .expect("payload");
    assert_eq!(out.len(), TILE_BYTES);
    assert_eq!(out[3], 128);
    assert_eq!(out[TILE_BYTES - 1], 0);
}

#[test]
fn tile_mip_chain_has_nine_levels_shrinking_to_one_pixel() {
    let base = opaque_tile();
    let chain = tile_mip_chain(&base);
    assert_eq!(chain.len(), 9, "256 -> 128 -> ... -> 1 is nine levels");
    assert_eq!(
        chain[0].len(),
        TILE_BYTES,
        "level 0 is the base image, untouched"
    );
    assert_eq!(chain[1].len(), 128 * 128 * 4);
    assert_eq!(chain[4].len(), 16 * 16 * 4);
    assert_eq!(chain[8].len(), 4, "the last level is a single pixel");
}

/// Skipping the chain is only ever safe for a slot that already holds one — `sync_tiles`
/// decides that per tile via `may_skip_mips`. The levels this leaves unwritten are why: a slot
/// that never had them would be sampled as whatever the atlas happened to contain, which at a
/// far enough zoom-out is nothing at all.
#[test]
fn motion_upload_skips_mip_chain() {
    let base = opaque_tile();
    assert!(
        tile_upload_mips(&base, true).is_empty(),
        "motion writes the base level and nothing above it"
    );
    assert!(
        !tile_upload_mips(&base, false).is_empty(),
        "a settled upload still has to carry every level"
    );
    assert_eq!(
        tile_mip_chain(&base).len(),
        tile_upload_mips(&base, false).len() + 1,
        "the chain is the base level plus the mips, and the base is never copied twice"
    );
    assert_eq!(tile_mip_chain(&base)[0].len(), TILE_BYTES);
}

#[test]
fn downsampling_a_flat_color_tile_keeps_the_color_at_every_level() {
    let mut base = vec![0u8; TILE_BYTES];
    for px in base.chunks_exact_mut(4) {
        px.copy_from_slice(&[10, 20, 30, 255]);
    }
    for level in tile_mip_chain(&base) {
        assert!(
            level.chunks_exact(4).all(|px| px == [10, 20, 30, 255]),
            "a uniform tile should downsample to the same uniform color"
        );
    }
}

#[test]
fn a_fully_transparent_tile_downsamples_to_fully_transparent() {
    let base = vec![0u8; TILE_BYTES];
    for level in tile_mip_chain(&base) {
        assert!(level.iter().all(|&b| b == 0));
    }
}

#[test]
fn transparent_pixels_do_not_bleed_color_into_an_opaque_neighbour() {
    // A vertical seam one pixel off the middle of a 2x2 downsample block, so the two source
    // texels that feed one destination pixel land on opposite sides of it: one fully
    // transparent, one opaque red.
    let side = TILE_SIZE as usize;
    let mut base = vec![0u8; TILE_BYTES];
    for y in 0..side {
        for x in 0..side {
            let i = (y * side + x) * 4;
            if x < 127 {
                base[i..i + 4].copy_from_slice(&[0, 0, 0, 0]);
            } else {
                base[i..i + 4].copy_from_slice(&[255, 0, 0, 255]);
            }
        }
    }
    let level1 = &tile_mip_chain(&base)[1]; // 128x128, straddling pixel at dx=63
    let i = 63 * 4;
    let px = &level1[i..i + 4];
    assert_eq!(
        &px[0..3],
        &[255, 0, 0],
        "color must come only from the opaque tap, not diluted by an invisible neighbour"
    );
    assert!(
        (110..145).contains(&px[3]),
        "alpha reflects roughly half coverage: got {}",
        px[3]
    );
}

#[test]
fn a_mask_shorter_than_the_document_leaves_pixels_past_its_end_untouched() {
    let doc_width = TILE_SIZE;
    let mut layer = paint_layer();
    // Only the first row is covered; every row below computes a mask index past
    // the buffer's end, which `mask.get` must skip rather than wrap or panic on.
    layer.set_mask(Some(vec![0u8; doc_width as usize]));

    let out = composited_tile_payload(
        &opaque_tile(),
        TileCoord { x: 0, y: 0 },
        &layer,
        None,
        doc_width,
    )
    .expect("payload");

    assert_eq!(out[3], 0, "row 0 is covered by the mask");
    let last_row_alpha = (((TILE_SIZE - 1) * TILE_SIZE) * 4 + 3) as usize;
    assert_eq!(
        out[last_row_alpha], 255,
        "past the mask's end, alpha is left as it was"
    );
}

#[test]
fn text_overlay_instances_is_empty_without_a_text_box() {
    let doc = Document::new("t".to_string(), "NoText", 64, 64);
    assert!(text_overlay_instances(&doc, 0.0).is_empty());
}

#[test]
fn text_overlay_draws_a_four_edge_box_and_a_caret_that_blinks() {
    let mut doc = Document::new("t".to_string(), "Text", 128, 128);
    doc.resize_viewport(128.0, 128.0, 1.0);
    doc.fit_to_view();
    doc.tool = Tool::Text;
    let (sx, sy) = doc.camera.to_screen(20.0, 20.0);
    doc.pointer_down(sx, sy);
    assert!(
        doc.text_editing(),
        "clicking with the text tool opens a session"
    );

    let visible = text_overlay_instances(&doc, 0.0);
    assert_eq!(visible.len(), 5, "four box edges plus a visible caret");
    for edge in &visible[0..4] {
        assert_eq!(edge.brush[0], 0.5, "box edges use the hairline width");
    }
    assert_eq!(visible[4].brush[0], 1.0, "the caret is drawn thicker");

    let hidden = text_overlay_instances(&doc, 0.7);
    assert_eq!(hidden.len(), 4, "mid-cycle the caret blinks off");
}

#[test]
fn brush_params_hand_the_shader_the_engines_own_profile() {
    let crayon = calumma_core::Brush::Crayon.profile();
    let params = brush_params(7.5, &crayon);

    assert_eq!(params[0], 7.5);
    assert_eq!(params[1], crayon.hardness);
    assert_eq!(params[2], crayon.grain);
    assert_eq!(params[3], crayon.grain_scale);
    assert!(crayon.grain > 0.0, "a smooth profile would not prove much");
}

#[test]
fn segment_count_matches_what_stroke_instances_actually_emits() {
    let color = [1.0, 1.0, 1.0, 1.0];
    for n in 0..5 {
        let points: Vec<StrokePoint> = (0..n)
            .map(|i| StrokePoint {
                x: i as f32,
                y: 0.0,
            })
            .collect();
        assert_eq!(
            stroke_instances(&points, 1.0, color, &BrushProfile::HARD).len(),
            stroke_segment_count(n),
            "n={n}"
        );
    }
}

/// A live stroke appends: each frame hands the GPU only the capsules the pointer has drawn
/// since the last one, and segment `i` must always mean the same capsule so the GPU's existing
/// coverage stays valid.
#[test]
fn resuming_a_stroke_emits_the_tail_and_nothing_already_drawn() {
    let color = [0.0, 0.0, 0.0, 1.0];
    let points: Vec<StrokePoint> = (0..5)
        .map(|i| StrokePoint {
            x: i as f32,
            y: 0.0,
        })
        .collect();
    let all = stroke_instances(&points, 1.0, color, &BrushProfile::HARD);

    let tail = stroke_instances_from(&points, 2, 1.0, color, &BrushProfile::HARD);

    assert_eq!(tail.len(), 2);
    assert_eq!(tail, all[2..]);
}

#[test]
fn resuming_past_the_last_segment_emits_nothing_rather_than_wrapping() {
    let color = [0.0, 0.0, 0.0, 1.0];
    let points = vec![
        StrokePoint { x: 0.0, y: 0.0 },
        StrokePoint { x: 1.0, y: 0.0 },
    ];
    assert!(stroke_instances_from(&points, 1, 1.0, color, &BrushProfile::HARD).is_empty());
    assert!(stroke_instances_from(&[], 0, 1.0, color, &BrushProfile::HARD).is_empty());
}

/// The one-point case is a degenerate capsule that segment 0 later replaces, so a caller that
/// appended across that boundary would draw the first dab twice.
#[test]
fn a_lone_point_is_segment_zero_and_is_replaced_rather_than_appended_to() {
    let color = [0.0, 0.0, 0.0, 1.0];
    let one = vec![StrokePoint { x: 3.0, y: 4.0 }];
    let dot = stroke_instances_from(&one, 0, 2.0, color, &BrushProfile::HARD);
    assert_eq!(dot.len(), 1);
    assert_eq!(dot[0].segment, [3.0, 4.0, 3.0, 4.0]);

    let two = vec![
        StrokePoint { x: 3.0, y: 4.0 },
        StrokePoint { x: 9.0, y: 4.0 },
    ];
    let first = stroke_instances_from(&two, 0, 2.0, color, &BrushProfile::HARD);
    assert_eq!(first[0].segment, [3.0, 4.0, 9.0, 4.0]);
}

#[test]
fn guides_span_the_paper_and_nothing_beyond_it() {
    let mut doc = Document::new("t".to_string(), "Guides", 400, 200);
    doc.resize_viewport(400.0, 200.0, 1.0);
    doc.add_guide(calumma_core::GuideAxis::Horizontal, 50.0)
        .expect("guide");
    doc.add_guide(calumma_core::GuideAxis::Vertical, 120.0)
        .expect("guide");

    let instances = guide_instances(&doc);

    assert_eq!(instances[0].segment, [0.0, 50.0, 400.0, 50.0]);
    assert_eq!(instances[1].segment, [120.0, 0.0, 120.0, 200.0]);
}

/// The guide under the pointer is drawn at full alpha so the one being moved is legible
/// against the ones it is being lined up with.
#[test]
fn the_dragged_guide_is_the_only_one_drawn_at_full_strength() {
    let mut doc = Document::new("t".to_string(), "Guides", 400, 400);
    doc.resize_viewport(400.0, 400.0, 1.0);
    doc.camera.zoom = 1.0;
    doc.camera.pan_x = 0.0;
    doc.camera.pan_y = 0.0;
    doc.add_guide(calumma_core::GuideAxis::Horizontal, 40.0)
        .expect("guide");
    let dragged = doc
        .add_guide(calumma_core::GuideAxis::Horizontal, 200.0)
        .expect("guide");
    let (sx, sy) = doc.camera.to_screen(10.0, 200.0);
    assert!(
        doc.begin_guide_drag(sx, sy),
        "the drag has to actually start"
    );
    assert_eq!(doc.dragged_guide(), Some(dragged));

    let instances = guide_instances(&doc);

    assert_eq!(instances[dragged].color[3], 1.0);
    assert!(instances[0].color[3] < 1.0);
    assert_eq!(instances[0].color[0..3], instances[dragged].color[0..3]);
}

#[test]
fn no_guides_means_no_instances() {
    let doc = Document::new("t".to_string(), "Guides", 64, 64);
    assert!(guide_instances(&doc).is_empty());
}

#[test]
fn mask_selection_edges_come_straight_off_the_traced_outline() {
    let mut doc = Document::new("t".to_string(), "Mask", 64, 64);
    assert!(selection_mask_edges(&doc, 1.0, [1.0; 4]).is_none());

    let mut mask = calumma_core::selection_mask::SelectionMask::new((0, 0), 64, 64);
    for y in 10..14 {
        for x in 10..14 {
            mask.set(x, y);
        }
    }
    let mask = mask.finish().expect("mask");
    let outline = mask.outline().len();
    doc.selection = Some(Selection {
        shape: SelectionShape::Mask(mask),
    });

    let edges = selection_mask_edges(&doc, 2.5, [0.0, 1.0, 0.0, 1.0]).expect("edges");

    assert_eq!(edges.len(), outline);
    assert_eq!(edges.len(), 4, "a square traces as one run per side");
    assert!(edges.iter().all(|e| e.brush[0] == 2.5));
    assert!(edges.iter().all(|e| e.color == [0.0, 1.0, 0.0, 1.0]));
}

#[test]
fn a_rect_selection_has_no_mask_edges_to_draw() {
    let mut doc = Document::new("t".to_string(), "Mask", 64, 64);
    doc.selection = Some(Selection {
        shape: SelectionShape::Rect {
            start: (0.0, 0.0),
            end: (4.0, 4.0),
        },
    });
    assert!(selection_mask_edges(&doc, 1.0, [1.0; 4]).is_none());
}

/// The box is four separate segments, so nothing enforces that they meet — a sign flip in one
/// corner draws three sides and a diagonal.
#[test]
fn the_text_box_is_drawn_as_a_closed_four_edge_loop() {
    let mut doc = Document::new("t".to_string(), "Text", 128, 128);
    doc.resize_viewport(128.0, 128.0, 1.0);
    doc.fit_to_view();
    doc.tool = Tool::Text;
    let (sx, sy) = doc.camera.to_screen(20.0, 20.0);
    doc.pointer_down(sx, sy);

    let edges = text_overlay_instances(&doc, 0.7);

    for i in 0..4 {
        let [_, _, x1, y1] = edges[i].segment;
        let [x0, y0, _, _] = edges[(i + 1) % 4].segment;
        assert_eq!(
            (x1, y1),
            (x0, y0),
            "edge {i} has to end where the next starts"
        );
    }
}
