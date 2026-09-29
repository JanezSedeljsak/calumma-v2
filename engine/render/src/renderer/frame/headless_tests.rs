use super::*;
use calumma_core::{BlendMode, Brush, Document, MemoryPressureLevel, Tool};

fn doc(w: u32, h: u32) -> Document {
    let mut doc = Document::new("p".into(), "t", w, h);
    doc.resize_viewport(256.0, 256.0, 1.0);
    doc.fit_to_view();
    doc
}

fn renderer() -> Option<Renderer> {
    Renderer::new_headless(256, 256)
}

#[test]
fn headless_renderer_draws_an_empty_board() {
    let Some(mut r) = renderer() else {
        return;
    };
    let mut doc = doc(128, 128);
    r.render(&mut doc);
    assert!(r.cached_tile_count() <= 2);
}

#[test]
fn headless_renderer_uploads_painted_tiles() {
    let Some(mut r) = renderer() else {
        return;
    };
    let mut doc = doc(128, 128);
    doc.tool = Tool::Pen;
    doc.pointer_down(20.0, 20.0);
    doc.pointer_move(40.0, 40.0);
    doc.pointer_up(40.0, 40.0);
    r.invalidate();
    r.render(&mut doc);
    assert!(r.cached_tile_count() > 0);
    assert!(r.gpu_tile_bytes() > 0);
}

#[test]
fn headless_renderer_follows_camera_motion_and_pressure() {
    let Some(mut r) = renderer() else {
        return;
    };
    let mut doc = doc(256, 256);
    r.render(&mut doc);
    r.begin_camera_motion();
    doc.camera.pan_x += 32.0;
    r.invalidate_camera();
    r.render(&mut doc);
    r.end_camera_motion();
    r.set_memory_pressure(MemoryPressureLevel::Critical);
    r.request_overview_prewarm();
    let _hint = r.frame_hint(&doc);
    r.release_document();
    assert_eq!(r.cached_tile_count(), 0);
}

#[test]
fn headless_renderer_draws_vectors_guides_and_previews() {
    let Some(mut r) = renderer() else {
        return;
    };
    let mut doc = doc(256, 256);
    doc.add_guide(calumma_core::GuideAxis::Horizontal, 40.0);
    doc.set_vector_mode(true);
    doc.tool = Tool::Rect;
    doc.pointer_down(30.0, 30.0);
    doc.pointer_move(90.0, 90.0);
    r.invalidate_overlay();
    r.render(&mut doc);
    doc.pointer_up(90.0, 90.0);
    r.invalidate();
    r.render(&mut doc);

    doc.tool = Tool::Pen;
    doc.brush = Brush::Airbrush;
    doc.pointer_down(10.0, 10.0);
    doc.pointer_move(50.0, 50.0);
    r.invalidate_overlay();
    r.render(&mut doc);

    doc.tool = Tool::Eraser;
    doc.pointer_move(55.0, 55.0);
    r.render(&mut doc);
}

#[test]
fn headless_renderer_handles_blend_modes_and_layer_props() {
    let Some(mut r) = renderer() else {
        return;
    };
    let mut doc = doc(128, 128);
    let layer = doc.active_layer;
    doc.layers[layer].blend_mode = BlendMode::Multiply;
    doc.layers[layer].opacity = 0.5;
    doc.layers[layer].adjustments = Some(calumma_core::Adjustments {
        brightness: 0.1,
        contrast: 0.0,
        vibrance: 0.0,
        saturation: 0.0,
        levels_gamma: 1.0,
        hue: 0.0,
    });
    doc.tool = Tool::Pen;
    doc.pointer_down(8.0, 8.0);
    doc.pointer_move(40.0, 40.0);
    doc.pointer_up(40.0, 40.0);
    r.invalidate();
    r.render(&mut doc);
    r.resize(320, 240);
    r.invalidate_overlay();
    r.render(&mut doc);
}

/// Three layers, each painted across a fifth of the visible tiles (well under the 48-tile
/// enter threshold on its own) but summing past it together. The old sum-across-layers gate
/// would have entered the overview here; the busiest single layer never asked for one.
#[test]
fn the_overview_gate_reads_the_busiest_layer_not_the_stack_total() {
    use calumma_core::limits::OVERVIEW_ENTER_TILE_THRESHOLD;
    use calumma_core::tile::TILE_SIZE;

    let Some(r) = renderer() else {
        return;
    };
    let mut d = doc(2048, 2048);
    for name in ["A", "B", "C"] {
        d.add_layer(name);
        let i = d.layers.len() - 1;
        let grid = d.layers[i].tiles_mut().unwrap();
        for ty in 0..4 {
            for tx in 0..5 {
                grid.set_pixel(
                    (tx * TILE_SIZE) as i32,
                    (ty * TILE_SIZE) as i32,
                    [1, 2, 3, 255],
                );
            }
        }
    }

    let busiest = r.busiest_layer_tile_count(&d);
    assert_eq!(busiest, 20, "one layer's own 4x5 painted block: {busiest}");
    assert!(
        busiest < OVERVIEW_ENTER_TILE_THRESHOLD,
        "no single layer alone needs the overview: {busiest}"
    );

    let summed_across_layers = busiest * 3;
    assert!(
        summed_across_layers >= OVERVIEW_ENTER_TILE_THRESHOLD,
        "the old sum-across-layers count would have crossed the threshold here \
         ({summed_across_layers}) even though the fix must not"
    );
}

#[test]
fn headless_renderer_zooms_out_into_overview() {
    let Some(mut r) = renderer() else {
        return;
    };
    let mut doc = doc(2048, 2048);
    doc.camera.zoom = 0.05;
    doc.tool = Tool::Pen;
    doc.pointer_down(100.0, 100.0);
    doc.pointer_move(200.0, 200.0);
    doc.pointer_up(200.0, 200.0);
    r.invalidate();
    r.render(&mut doc);
    doc.camera.zoom = 0.01;
    r.invalidate_camera();
    r.render(&mut doc);
    doc.pointer_down(400.0, 400.0);
    doc.pointer_move(500.0, 480.0);
    doc.pointer_up(500.0, 480.0);
    r.invalidate();
    r.render(&mut doc);
}

#[test]
fn headless_renderer_pushes_a_vector_item_layer() {
    let Some(mut r) = renderer() else {
        return;
    };
    let mut doc = doc(256, 256);
    doc.set_vector_mode(true);
    doc.tool = Tool::Rect;
    doc.pointer_down(20.0, 20.0);
    doc.pointer_move(80.0, 80.0);
    doc.pointer_up(80.0, 80.0);
    r.invalidate();
    r.render(&mut doc);
}

#[test]
fn headless_renderer_text_caret_and_selection_overlay() {
    let Some(mut r) = renderer() else {
        return;
    };
    let mut doc = doc(256, 256);
    doc.tool = Tool::Text;
    doc.pointer_down(40.0, 40.0);
    doc.text_insert("hello");
    r.invalidate_overlay();
    r.render(&mut doc);
    r.render(&mut doc);

    doc.tool = Tool::SelectRect;
    doc.pointer_down(60.0, 60.0);
    doc.pointer_move(120.0, 120.0);
    r.invalidate_overlay();
    r.render(&mut doc);
}

#[test]
fn headless_renderer_clone_and_transform_overlays() {
    let Some(mut r) = renderer() else {
        return;
    };
    let mut doc = doc(256, 256);
    doc.tool = Tool::Pen;
    doc.pointer_down(30.0, 30.0);
    doc.pointer_move(80.0, 80.0);
    doc.pointer_up(80.0, 80.0);
    doc.tool = Tool::Clone;
    doc.set_clone_anchor(40.0, 50.0);
    let (sx, sy) = doc.camera.to_screen(80.0, 60.0);
    doc.set_pointer_hover(sx, sy);
    r.invalidate_overlay();
    r.render(&mut doc);

    doc.enter_transform();
    r.invalidate_overlay();
    r.render(&mut doc);
}

#[test]
fn crop_and_transform_chrome_survive_a_camera_only_pan() {
    let Some(mut r) = renderer() else {
        return;
    };
    let mut doc = doc(256, 256);
    doc.set_tool(Tool::Crop);
    r.invalidate();
    r.render(&mut doc);
    let crop_at_rest = r.screen_overlay_scratch.len();
    assert!(
        crop_at_rest > 0,
        "crop chrome is on the board before the pan"
    );

    r.begin_camera_motion();
    doc.camera.pan_x += 40.0;
    r.invalidate_camera();
    r.render(&mut doc);
    assert_eq!(
        r.screen_overlay_scratch.len(),
        crop_at_rest,
        "crop chrome is rewritten on a pan, not dropped"
    );
    r.end_camera_motion();

    doc.set_tool(Tool::Pen);
    doc.pointer_down(30.0, 30.0);
    doc.pointer_move(80.0, 80.0);
    doc.pointer_up(80.0, 80.0);
    assert!(doc.enter_transform());
    r.invalidate();
    r.render(&mut doc);
    let transform_at_rest = r.screen_overlay_scratch.len();
    assert!(
        transform_at_rest > 0,
        "transform chrome is on the board before the pan"
    );

    r.begin_camera_motion();
    doc.camera.pan_y += 24.0;
    r.invalidate_camera();
    r.render(&mut doc);
    assert_eq!(
        r.screen_overlay_scratch.len(),
        transform_at_rest,
        "transform chrome is rewritten on a pan, not dropped"
    );
    r.end_camera_motion();
}

#[test]
fn headless_renderer_dark_theme_and_shape_preview() {
    let Some(mut r) = renderer() else {
        return;
    };
    let mut doc = doc(256, 256);
    doc.dark_theme = true;
    doc.fill = true;
    doc.stroke = true;
    doc.tool = Tool::Ellipse;
    doc.pointer_down(40.0, 40.0);
    doc.pointer_move(120.0, 120.0);
    r.invalidate_overlay();
    r.render(&mut doc);
    doc.pointer_up(120.0, 120.0);
    r.invalidate();
    r.render(&mut doc);
}

#[test]
fn headless_renderer_lasso_selection_and_pan_cache() {
    let Some(mut r) = renderer() else {
        return;
    };
    let mut doc = doc(512, 512);
    doc.camera.zoom = 2.0;
    doc.tool = Tool::Pen;
    doc.pointer_down(40.0, 40.0);
    doc.pointer_move(100.0, 100.0);
    doc.pointer_up(100.0, 100.0);
    r.invalidate();
    r.render(&mut doc);

    r.begin_camera_motion();
    doc.camera.pan_x += 48.0;
    r.invalidate_camera();
    r.render(&mut doc);
    r.end_camera_motion();

    doc.tool = Tool::SelectLasso;
    doc.pointer_down(60.0, 60.0);
    doc.pointer_move(80.0, 90.0);
    doc.pointer_move(110.0, 70.0);
    r.invalidate_overlay();
    r.render(&mut doc);
}

#[test]
fn headless_renderer_heal_and_fill_tools() {
    let Some(mut r) = renderer() else {
        return;
    };
    let mut doc = doc(256, 256);
    doc.tool = Tool::Pen;
    doc.pointer_down(20.0, 20.0);
    doc.pointer_move(60.0, 60.0);
    doc.pointer_up(60.0, 60.0);
    doc.tool = Tool::Heal;
    doc.set_clone_anchor(30.0, 30.0);
    doc.pointer_down(80.0, 80.0);
    doc.pointer_move(100.0, 100.0);
    r.invalidate_overlay();
    r.render(&mut doc);
    doc.pointer_up(100.0, 100.0);
    r.invalidate();
    r.render(&mut doc);

    doc.tool = Tool::Fill;
    doc.pointer_down(50.0, 50.0);
    r.render(&mut doc);
}

#[test]
fn headless_renderer_stacked_layers_and_screen_blend() {
    let Some(mut r) = renderer() else {
        return;
    };
    let mut doc = doc(256, 256);
    doc.tool = Tool::Pen;
    doc.pointer_down(10.0, 10.0);
    doc.pointer_move(80.0, 80.0);
    doc.pointer_up(80.0, 80.0);
    doc.add_layer("Ink");
    doc.layers[doc.active_layer].blend_mode = BlendMode::Screen;
    doc.tool = Tool::Pen;
    doc.pointer_down(30.0, 30.0);
    doc.pointer_move(90.0, 90.0);
    doc.pointer_up(90.0, 90.0);
    r.invalidate();
    r.render(&mut doc);
}
