use calumma_core::document::*;
use calumma_core::paste::PasteOutcome;
use calumma_core::*;

fn board() -> Document {
    let mut doc = Document::new("p".into(), "t", 256, 256);
    doc.resize_viewport(256.0, 256.0, 1.0);
    doc.fit_to_view();
    doc
}

fn pixel(doc: &Document, x: i32, y: i32) -> [u8; 4] {
    doc.layers[doc.active_layer]
        .tiles()
        .unwrap()
        .get_pixel(x, y)
}

/// A slow drag: many points a fraction of a pixel apart, which is what a real pointer sends
/// and what used to make every stamp overlap its neighbours almost completely.
fn slow_drag(doc: &mut Document, from: (f32, f32), to: (f32, f32), steps: usize) {
    let (sx, sy) = doc.camera.to_screen(from.0, from.1);
    doc.pointer_down(sx, sy);
    for step in 1..=steps {
        let t = step as f32 / steps as f32;
        let x = from.0 + (to.0 - from.0) * t;
        let y = from.1 + (to.1 - from.1) * t;
        let (mx, my) = doc.camera.to_screen(x, y);
        doc.pointer_move(mx, my);
    }
    let (ex, ey) = doc.camera.to_screen(to.0, to.1);
    doc.pointer_up(ex, ey);
}

/// The bug the whole coverage model exists to kill. A stroke used to blend each stamp onto the
/// last, so at any opacity below 1 the overlaps compounded and a slow drag came out as a dark
/// beaded rope that got darker the slower you drew. One stroke is now one wash: every pixel
/// the pen fully covered ends up at exactly the same alpha, however many segments crossed it.
#[test]
fn a_low_opacity_stroke_is_one_even_wash() {
    let mut doc = board();
    doc.tool = Tool::Pen;
    doc.set_color([0, 0, 0, 255]);
    doc.set_ink_opacity(0.25);
    doc.brush_size = 12.0;

    slow_drag(&mut doc, (40.0, 128.0), (200.0, 128.0), 300);

    let along: Vec<u8> = (60..180).map(|x| pixel(&doc, x, 128)[3]).collect();
    let low = *along.iter().min().unwrap();
    let high = *along.iter().max().unwrap();
    assert!(low > 0, "the stroke landed");
    assert_eq!(low, high, "and it is flat along its length: {low}..{high}");
    assert!(
        (high as i32 - 64).abs() <= 2,
        "at a quarter opacity it sits near a quarter alpha, not saturated: {high}"
    );
}

/// Drawing over the same place *again* is a second stroke, and a second stroke does build up —
/// that is how you deepen a wash. Only the compounding *within* one stroke was wrong.
#[test]
fn a_second_pass_still_builds_up() {
    let mut doc = board();
    doc.tool = Tool::Pen;
    doc.set_color([0, 0, 0, 255]);
    doc.set_ink_opacity(0.25);
    doc.brush_size = 12.0;

    slow_drag(&mut doc, (40.0, 128.0), (200.0, 128.0), 60);
    let once = pixel(&doc, 120, 128)[3];
    slow_drag(&mut doc, (40.0, 128.0), (200.0, 128.0), 60);
    let twice = pixel(&doc, 120, 128)[3];

    assert!(
        twice > once,
        "a second pass deepens it: {once} then {twice}"
    );
}

/// `Brush::Pen` is the brush Calumma always had, and picking it must not change a single pixel
/// of what a stroke used to lay down — a hard edge feathered over exactly one pixel.
#[test]
fn the_pen_brush_keeps_a_hard_edge() {
    let mut doc = board();
    doc.tool = Tool::Pen;
    doc.set_brush(Brush::Pen);
    doc.set_color([0, 0, 0, 255]);
    doc.brush_size = 20.0;

    slow_drag(&mut doc, (60.0, 128.0), (180.0, 128.0), 40);

    assert_eq!(pixel(&doc, 120, 128)[3], 255, "solid at the centre");
    assert_eq!(
        pixel(&doc, 120, 137)[3],
        255,
        "and still solid one pixel inside the rim"
    );
    assert_eq!(pixel(&doc, 120, 140)[3], 0, "nothing past the rim");
}

/// The point of a marker and an airbrush: they lay down less ink than the pen at the same
/// color and the same opacity slider, so they read as translucent without any fiddling.
#[test]
fn brushes_lay_down_their_own_amount_of_ink() {
    let laid = |brush: Brush| {
        let mut doc = board();
        doc.tool = Tool::Pen;
        doc.set_brush(brush);
        doc.set_color([0, 0, 0, 255]);
        doc.brush_size = 20.0;
        slow_drag(&mut doc, (60.0, 128.0), (180.0, 128.0), 40);
        pixel(&doc, 120, 128)[3]
    };

    let pen = laid(Brush::Pen);
    let marker = laid(Brush::Marker);
    let crayon = laid(Brush::Crayon);
    let airbrush = laid(Brush::Airbrush);

    assert_eq!(pen, 255, "the pen is opaque ink");
    assert!(
        marker < pen && crayon < pen && airbrush < marker,
        "each brush is progressively lighter: pen {pen}, marker {marker}, \
         crayon {crayon}, airbrush {airbrush}"
    );
    assert!(airbrush > 0, "the airbrush still lands something");
}

/// A soft brush fades out instead of stopping at a rim, but it must not spread wider than the
/// brush size says it is — the falloff reaches inward from the edge, so every brush covers the
/// same width and the size slider keeps one meaning.
#[test]
fn a_soft_brush_fades_without_growing() {
    let mut doc = board();
    doc.tool = Tool::Pen;
    doc.set_brush(Brush::Airbrush);
    doc.set_color([0, 0, 0, 255]);
    doc.brush_size = 20.0;

    slow_drag(&mut doc, (60.0, 128.0), (180.0, 128.0), 40);

    let centre = pixel(&doc, 120, 128)[3];
    let midway = pixel(&doc, 120, 133)[3];
    let rim = pixel(&doc, 120, 138)[3];
    assert!(
        centre > midway && midway > rim,
        "it falls off across its width: {centre} / {midway} / {rim}"
    );
    assert_eq!(pixel(&doc, 120, 141)[3], 0, "and stops at the same rim");
}

/// Crayon grain is keyed on document position, not on where along the stroke a pixel is, so
/// the tooth belongs to the paper: two strokes crossing the same spot bite in the same places.
#[test]
fn crayon_grain_is_fixed_to_the_paper() {
    let sample = |from: (f32, f32), to: (f32, f32)| {
        let mut doc = board();
        doc.tool = Tool::Pen;
        doc.set_brush(Brush::Crayon);
        doc.set_color([0, 0, 0, 255]);
        doc.brush_size = 24.0;
        slow_drag(&mut doc, from, to, 60);
        let row: Vec<u8> = (100..140).map(|x| pixel(&doc, x, 128)[3]).collect();
        row
    };

    let rightwards = sample((60.0, 128.0), (180.0, 128.0));
    let leftwards = sample((180.0, 128.0), (60.0, 128.0));
    assert_eq!(
        rightwards, leftwards,
        "the grain does not slide along with the stroke"
    );

    let low = *rightwards.iter().min().unwrap();
    let high = *rightwards.iter().max().unwrap();
    assert!(high - low > 20, "and it actually bites: {low}..{high}");
}

/// The eraser carries its own edge, not the pen's brush: picking Airbrush for the pen must
/// leave the eraser exactly as hard as it was. Grain and flow describe ink going down, and the
/// eraser is taking it away.
#[test]
fn the_eraser_ignores_the_pens_brush() {
    let mut doc = board();
    {
        let tiles = doc.layers[doc.active_layer].tiles_mut().unwrap();
        tiles.fill_uniform(DocRect::new(0, 0, 255, 255), [10, 20, 30, 255]);
    }
    doc.tool = Tool::Eraser;
    doc.set_brush(Brush::Airbrush);
    doc.brush_size = 20.0;

    slow_drag(&mut doc, (60.0, 128.0), (180.0, 128.0), 40);

    assert_eq!(pixel(&doc, 120, 128), [0, 0, 0, 0], "erased outright");
    assert_eq!(
        pixel(&doc, 120, 137),
        [0, 0, 0, 0],
        "right up to the rim, however the pen's brush is set"
    );
    assert_eq!(pixel(&doc, 120, 140), [10, 20, 30, 255], "and no further");
}

/// A brush changes how a stroke lands, never how it is undone.
#[test]
fn a_brush_stroke_is_still_one_undo() {
    let mut doc = board();
    doc.tool = Tool::Pen;
    doc.set_brush(Brush::Crayon);
    doc.set_color([0, 0, 0, 255]);
    doc.brush_size = 16.0;

    slow_drag(&mut doc, (60.0, 128.0), (180.0, 128.0), 80);
    assert_ne!(pixel(&doc, 120, 128), [0, 0, 0, 0]);

    doc.undo();
    assert_eq!(pixel(&doc, 120, 128), [0, 0, 0, 0]);
    assert!(!doc.history.can_undo(), "one entry for the whole stroke");
}

/// A tap is a dab, not nothing: a stroke of one point still has to lay ink down.
#[test]
fn a_single_tap_lands_a_dab() {
    let mut doc = board();
    doc.tool = Tool::Pen;
    doc.set_color([0, 0, 0, 255]);
    doc.brush_size = 16.0;

    let (sx, sy) = doc.camera.to_screen(128.0, 128.0);
    doc.pointer_down(sx, sy);
    doc.pointer_up(sx, sy);

    assert_eq!(pixel(&doc, 128, 128)[3], 255);
    assert_eq!(pixel(&doc, 128, 100)[3], 0);
}

/// Vector mode turns a pen stroke into a resolution-independent path, which has no raster
/// coverage to shape — so the brush does not apply there, and the shell hides the picker.
#[test]
fn vector_mode_ignores_the_brush() {
    let mut doc = board();
    doc.tool = Tool::Pen;
    doc.vector_mode = true;
    doc.set_brush(Brush::Airbrush);
    assert!(
        !doc.previews_brush_stroke(),
        "a vector pen previews as a plain path, not through the coverage pass"
    );

    doc.set_color([0, 0, 0, 255]);
    slow_drag(&mut doc, (60.0, 128.0), (180.0, 128.0), 20);
    assert!(
        doc.layers[doc.active_layer].content.item().is_some(),
        "it committed a vector item"
    );
}

/// The point of the knob. At full softness the rim feathers instead of cutting, so an erase
/// blends into what it is eating away at rather than leaving a stamped-out hole.
#[test]
fn a_soft_eraser_feathers_its_rim() {
    let mut doc = board();
    {
        let tiles = doc.layers[doc.active_layer].tiles_mut().unwrap();
        tiles.fill_uniform(DocRect::new(0, 0, 255, 255), [10, 20, 30, 255]);
    }
    doc.tool = Tool::Eraser;
    doc.set_eraser_hardness(0.0);
    doc.brush_size = 20.0;

    slow_drag(&mut doc, (60.0, 128.0), (180.0, 128.0), 40);

    let centre = pixel(&doc, 120, 128);
    let midway = pixel(&doc, 120, 133)[3];
    let rim = pixel(&doc, 120, 138)[3];
    assert_eq!(centre, [0, 0, 0, 0], "the middle is erased outright");
    assert!(
        midway > 0 && midway < 255,
        "the rim is partly erased, not cut: {midway}"
    );
    assert!(
        rim > midway,
        "and it fades back to untouched on the way out: {midway} then {rim}"
    );
    assert_eq!(
        pixel(&doc, 120, 141),
        [10, 20, 30, 255],
        "stopping at the same rim a hard eraser would"
    );
}

/// A soft erase keeps the color it is thinning out. Alpha comes down, RGB stays put — tiles
/// hold straight alpha, so zeroing the channels would turn a half-erased edge black.
#[test]
fn a_soft_erase_thins_alpha_without_touching_color() {
    let mut doc = board();
    {
        let tiles = doc.layers[doc.active_layer].tiles_mut().unwrap();
        tiles.fill_uniform(DocRect::new(0, 0, 255, 255), [200, 40, 90, 255]);
    }
    doc.tool = Tool::Eraser;
    doc.set_eraser_hardness(0.0);
    doc.brush_size = 20.0;

    slow_drag(&mut doc, (60.0, 128.0), (180.0, 128.0), 40);

    let edge = pixel(&doc, 120, 134);
    assert!(edge[3] > 0 && edge[3] < 255, "partly erased: {edge:?}");
    assert_eq!(
        [edge[0], edge[1], edge[2]],
        [200, 40, 90],
        "and still its own color"
    );
}

/// Coverage maxes within one stroke, so a single pass over the rim can only take it so far.
/// Going over it again eats further in — which is how a real soft eraser behaves.
#[test]
fn a_second_pass_erases_further() {
    let mut doc = board();
    {
        let tiles = doc.layers[doc.active_layer].tiles_mut().unwrap();
        tiles.fill_uniform(DocRect::new(0, 0, 255, 255), [10, 20, 30, 255]);
    }
    doc.tool = Tool::Eraser;
    doc.set_eraser_hardness(0.0);
    doc.brush_size = 20.0;

    slow_drag(&mut doc, (60.0, 128.0), (180.0, 128.0), 40);
    let once = pixel(&doc, 120, 134)[3];
    slow_drag(&mut doc, (60.0, 128.0), (180.0, 128.0), 40);
    let twice = pixel(&doc, 120, 134)[3];

    assert!(once > 0, "one pass leaves the rim standing");
    assert!(
        twice < once,
        "a second takes it further: {once} then {twice}"
    );
}

/// The default is the eraser Calumma has always had, so no existing file or habit changes.
#[test]
fn the_default_eraser_is_the_hard_one() {
    let mut doc = board();
    assert_eq!(doc.eraser_hardness, 1.0);
    {
        let tiles = doc.layers[doc.active_layer].tiles_mut().unwrap();
        tiles.fill_uniform(DocRect::new(0, 0, 255, 255), [10, 20, 30, 255]);
    }
    doc.tool = Tool::Eraser;
    doc.brush_size = 20.0;

    slow_drag(&mut doc, (60.0, 128.0), (180.0, 128.0), 40);

    assert_eq!(pixel(&doc, 120, 137), [0, 0, 0, 0], "hard to the rim");
    assert_eq!(pixel(&doc, 120, 140), [10, 20, 30, 255], "and no further");
}

/// Out-of-range values clamp rather than producing a rim that erases more than everything.
#[test]
fn eraser_hardness_clamps() {
    let mut doc = board();
    doc.set_eraser_hardness(-4.0);
    assert_eq!(doc.eraser_hardness, 0.0);
    doc.set_eraser_hardness(9.0);
    assert_eq!(doc.eraser_hardness, 1.0);
}

/// A layer that has been moved holds its pixels in its **own** grid, and the renderer maps that
/// grid into the document through the transform. So a stroke aimed at a document coordinate has
/// to be mapped the other way before it is stamped, or it lands in the grid at the document
/// position and the transform then carries it somewhere else — which is exactly what a pasted
/// image looked like: the stroke followed the pointer until pointer-up, then jumped.
#[test]
fn a_stroke_on_a_moved_layer_lands_where_it_was_drawn() {
    let mut doc = board();
    doc.add_layer("Pasted");
    let index = doc.active_layer;

    // Content first: the transform pivots on the layer's own painted bounds.
    if let Some(tiles) = doc.layers[index].tiles_mut() {
        for y in 10..30 {
            for x in 10..30 {
                tiles.set_pixel(x, y, [255, 255, 255, 255]);
            }
        }
    }
    doc.layers[index].transform = Some(LayerTransform {
        offset_x: 40.0,
        offset_y: 0.0,
        ..LayerTransform::default()
    });

    // The grid point (20, 20) is drawn at document (60, 20) once the offset is applied, so a
    // dot placed there has to come back out of the grid at (20, 20).
    doc.set_tool(Tool::Pen);
    doc.brush_size = 8.0;
    doc.color = [255, 0, 0, 255];
    doc.pointer_down(60.0, 20.0);
    doc.pointer_up(60.0, 20.0);

    assert_eq!(
        pixel(&doc, 20, 20),
        [255, 0, 0, 255],
        "stroke is in grid space"
    );
    assert_ne!(
        pixel(&doc, 60, 20),
        [255, 0, 0, 255],
        "and not at the document coordinate it was aimed at"
    );
}

/// A pasted image bigger than the paper opens the layer's extent past the document, and the part
/// hanging off is still part of the layer. The coverage used to be clipped to the *paper*, so a
/// stroke out there was rasterised into nothing and silently did not happen.
#[test]
fn a_stroke_lands_on_the_part_of_a_pasted_layer_that_hangs_off_the_paper() {
    let mut doc = Document::new("p".into(), "t", 64, 64);
    doc.resize_viewport(64.0, 64.0, 1.0);
    doc.fit_to_view();

    let side = 200usize;
    let image = vec![255u8; side * side * 4];
    assert_eq!(
        doc.paste_image_as_layer("Pasted", &image, side as u32, side as u32),
        PasteOutcome::Overflowing
    );

    // Centred, so the layer reaches from -68 to 131 on both axes. This point is well past the
    // paper's 64px edge and well inside the layer.
    let off_paper = (110.0f32, 32.0f32);
    doc.set_tool(Tool::Pen);
    doc.brush_size = 8.0;
    doc.color = [255, 0, 0, 255];
    doc.pointer_down(off_paper.0, off_paper.1);
    doc.pointer_up(off_paper.0, off_paper.1);

    assert_eq!(
        pixel(&doc, off_paper.0 as i32, off_paper.1 as i32),
        [255, 0, 0, 255],
        "the stroke has to reach the overflow"
    );
}
