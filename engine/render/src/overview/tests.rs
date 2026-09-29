use super::*;
use crate::test_gpu::{gpu, read_texture_2d, Gpu};
use calumma_core::tile::DocRect;
use calumma_core::vector::VectorShape;
use calumma_core::{Adjustments, DeviceTier, LayerTransform, MemoryPressureLevel, Shape, Tool};

fn pass(gpu: &Gpu) -> OverviewPass {
    OverviewPass::new(
        &gpu.device,
        &gpu.shader,
        wgpu::TextureFormat::Bgra8UnormSrgb,
    )
}

fn budget() -> GpuBudget {
    GpuBudget::new(DeviceTier::Standard)
}

fn doc(width: u32, height: u32) -> Document {
    Document::new("p".into(), "t", width, height)
}

fn sync(overview: &mut OverviewPass, doc: &mut Document, gpu: &Gpu) {
    overview.sync(doc, &gpu.device, &gpu.queue, &budget());
}

fn has_texture(overview: &OverviewPass) -> bool {
    overview
        .levels
        .get(overview.displayed)
        .is_some_and(|level| level.texture.is_some())
}

fn resident_textures(overview: &OverviewPass) -> usize {
    overview
        .levels
        .iter()
        .filter(|level| level.texture.is_some())
        .count()
}

fn paint(doc: &mut Document, rect: DocRect, rgba: [u8; 4]) {
    let layer = doc.active_layer;
    doc.layers[layer]
        .tiles_mut()
        .unwrap()
        .paint_rect(rect, |_, _, _| Some(rgba));
}

fn gpu_matches_flatten(overview: &OverviewPass, doc: &Document, gpu: &Gpu) {
    let level = overview
        .levels
        .get(overview.displayed)
        .and_then(|level| level.texture.as_ref().map(|texture| (level, texture)))
        .expect("displayed level");
    let (tw, th) = (level.0.tex_width, level.0.tex_height);
    let cpu = doc.composite_overview_rect(level.0.max_side, 0, 0, tw, th);
    let gpu_px = read_texture_2d(&gpu.device, &gpu.queue, level.1, tw, th);
    assert_eq!(gpu_px, cpu, "{}x{} flatten mismatch", tw, th);
}

#[test]
fn the_overview_switches_on_and_off_with_hysteresis() {
    let Some(gpu) = gpu() else { return };
    let mut overview = pass(gpu);

    assert!(!overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD - 1, false));
    assert!(overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false));
    assert!(
        overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD - 1, false),
        "once on, it stays on well below the entry threshold"
    );
    assert!(overview.should_use(OVERVIEW_EXIT_TILE_THRESHOLD + 1, false));
    assert!(!overview.should_use(OVERVIEW_EXIT_TILE_THRESHOLD, false));
}

#[test]
fn live_editing_takes_the_overview_off_whatever_the_tile_count() {
    let Some(gpu) = gpu() else { return };
    let mut overview = pass(gpu);
    assert!(overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false));

    assert!(!overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD * 10, true));
    assert!(
        !overview.should_use(OVERVIEW_EXIT_TILE_THRESHOLD + 1, false),
        "coming back off a stroke re-enters through the entry threshold, not the exit one"
    );
}

#[test]
fn an_inactive_pass_never_composites_the_document() {
    let Some(gpu) = gpu() else { return };
    let mut overview = pass(gpu);
    let mut d = doc(512, 512);

    sync(&mut overview, &mut d, gpu);

    assert!(!has_texture(&overview), "no upload while inactive");
    assert!(overview.dirty);
}

#[test]
fn an_active_pass_uploads_once_and_then_leaves_the_texture_alone() {
    let Some(gpu) = gpu() else { return };
    let mut overview = pass(gpu);
    let mut d = doc(512, 256);
    overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false);

    sync(&mut overview, &mut d, gpu);
    assert!(has_texture(&overview));
    assert!(!overview.dirty);
    assert_eq!((overview.doc_width, overview.doc_height), (512, 256));
    assert_eq!((overview.tex_width, overview.tex_height), (512, 256));

    let (w, h) = (overview.tex_width, overview.tex_height);
    sync(&mut overview, &mut d, gpu);
    assert_eq!((overview.tex_width, overview.tex_height), (w, h));
}

#[test]
fn a_document_resize_re_uploads_without_anything_marking_it_dirty() {
    let Some(gpu) = gpu() else { return };
    let mut overview = pass(gpu);
    overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false);
    sync(&mut overview, &mut doc(512, 256), gpu);

    sync(&mut overview, &mut doc(256, 512), gpu);

    assert_eq!((overview.doc_width, overview.doc_height), (256, 512));
    assert_eq!((overview.tex_width, overview.tex_height), (256, 512));
}

#[test]
fn marking_dirty_makes_the_next_sync_upload_again() {
    let Some(gpu) = gpu() else { return };
    let mut overview = pass(gpu);
    let mut d = doc(128, 128);
    overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false);
    sync(&mut overview, &mut d, gpu);

    overview.mark_dirty();
    assert!(overview.dirty);
    sync(&mut overview, &mut d, gpu);
    assert!(!overview.dirty);
}

#[test]
fn re_uploading_at_the_same_size_keeps_the_texture_it_already_had() {
    let Some(gpu) = gpu() else { return };
    let mut overview = pass(gpu);
    let mut d = doc(512, 256);
    overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false);
    sync(&mut overview, &mut d, gpu);
    assert_eq!(overview.allocations, 1);

    paint(&mut d, DocRect::new(10, 10, 20, 20), [255, 0, 0, 255]);
    sync(&mut overview, &mut d, gpu);

    assert_eq!(
        overview.allocations, 1,
        "a paint patches the texture it already had"
    );
    assert!(!overview.dirty);
    gpu_matches_flatten(&overview, &d, gpu);
}

#[test]
fn a_resize_replaces_the_texture_and_its_bind_group() {
    let Some(gpu) = gpu() else { return };
    let mut overview = pass(gpu);
    overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false);
    sync(&mut overview, &mut doc(512, 256), gpu);
    assert_eq!(overview.allocations, 1);

    sync(&mut overview, &mut doc(256, 512), gpu);

    assert_eq!(overview.allocations, 2);
    assert_eq!((overview.tex_width, overview.tex_height), (256, 512));
}

#[test]
fn prewarm_uploads_while_inactive_and_only_once_per_request() {
    let Some(gpu) = gpu() else { return };
    let mut overview = pass(gpu);
    let mut d = doc(128, 128);

    overview.prewarm(&mut d, &gpu.device, &gpu.queue, &budget());
    assert!(!has_texture(&overview), "nothing was requested yet");

    overview.request_prewarm();
    overview.prewarm(&mut d, &gpu.device, &gpu.queue, &budget());
    assert!(has_texture(&overview));
    assert!(!overview.prewarm_pending);
    assert!(!overview.active, "prewarming does not turn the overview on");

    overview.mark_dirty();
    overview.prewarm(&mut d, &gpu.device, &gpu.queue, &budget());
    assert!(overview.dirty, "a spent request does not upload again");
}

#[test]
fn clearing_drops_the_texture_and_the_pass_falls_back_to_drawing_nothing() {
    let Some(gpu) = gpu() else { return };
    let mut overview = pass(gpu);
    overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false);
    sync(&mut overview, &mut doc(128, 128), gpu);

    overview.clear();

    assert!(!has_texture(&overview));
    assert!(overview.levels.is_empty());
    assert!(!overview.active);
    assert!(overview.dirty);
    assert_eq!((overview.doc_width, overview.doc_height), (0, 0));
}

#[test]
fn the_camera_uniform_stays_the_size_the_shader_declares() {
    let Some(gpu) = gpu() else { return };
    let overview = pass(gpu);
    let mut d = doc(128, 128);
    d.camera.pan_x = 12.0;

    overview.write_camera(&gpu.queue, &d, [800.0, 600.0]);

    assert_eq!(std::mem::size_of::<OverviewCamera>(), 10 * 4);
}

#[test]
fn zooming_in_picks_a_finer_pyramid_level() {
    let Some(gpu) = gpu() else { return };
    let mut overview = pass(gpu);
    let mut d = doc(512, 512);
    d.resize_viewport(64.0, 64.0, 1.0);
    d.fit_to_view();
    overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false);
    sync(&mut overview, &mut d, gpu);
    let far = overview.tex_width;

    d.camera.zoom = 1.0;
    sync(&mut overview, &mut d, gpu);

    assert!(
        overview.tex_width >= far,
        "a closer camera must not pick a coarser flatten ({far} -> {})",
        overview.tex_width
    );
    assert_eq!(overview.tex_width, 512);
}

#[test]
fn painting_one_tile_reuses_the_texture_and_clears_overview_dirty() {
    let Some(gpu) = gpu() else { return };
    let mut overview = pass(gpu);
    let mut d = doc(512, 512);
    overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false);
    sync(&mut overview, &mut d, gpu);
    assert_eq!(overview.allocations, 1);
    assert!(d.layers[0]
        .dirty_tiles(DirtyChannel::Overview)
        .is_some_and(|set| set.is_empty()));

    let layer = d.active_layer;
    d.layers[layer]
        .tiles_mut()
        .unwrap()
        .paint_rect(DocRect::new(10, 10, 20, 20), |_, _, _| {
            Some([255, 0, 0, 255])
        });
    assert!(d.layers[layer]
        .dirty_tiles(DirtyChannel::Overview)
        .is_some_and(|set| !set.is_empty()));

    sync(&mut overview, &mut d, gpu);
    assert_eq!(overview.allocations, 1);
    assert!(d.layers[layer]
        .dirty_tiles(DirtyChannel::Overview)
        .is_some_and(|set| set.is_empty()));
    assert!(!overview.levels[overview.displayed].full_dirty);
    gpu_matches_flatten(&overview, &d, gpu);
}

#[test]
fn an_overview_flatten_does_not_clear_the_render_channel() {
    let Some(gpu) = gpu() else { return };
    let mut overview = pass(gpu);
    let mut d = doc(128, 128);
    overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false);
    assert!(d.layers[0]
        .dirty_tiles(DirtyChannel::Render)
        .is_some_and(|set| !set.is_empty()));

    sync(&mut overview, &mut d, gpu);

    assert!(d.layers[0]
        .dirty_tiles(DirtyChannel::Overview)
        .is_some_and(|set| set.is_empty()));
    assert!(d.layers[0]
        .dirty_tiles(DirtyChannel::Render)
        .is_some_and(|set| !set.is_empty()));
}

#[test]
fn an_odd_aspect_document_uploads_bytes_the_gpu_can_round_trip() {
    let Some(gpu) = gpu() else { return };
    let mut overview = pass(gpu);
    let mut d = doc(200, 300);
    overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false);
    sync(&mut overview, &mut d, gpu);
    gpu_matches_flatten(&overview, &d, gpu);
}

#[test]
fn zooming_out_allocates_a_coarser_level_and_zooming_in_reuses_the_fine_one() {
    let Some(gpu) = gpu() else { return };
    let mut overview = pass(gpu);
    let mut d = doc(2048, 2048);
    overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false);
    d.camera.zoom = 1.0;
    d.camera.dpr = 1.0;
    sync(&mut overview, &mut d, gpu);
    assert_eq!(overview.tex_width, 2048);
    assert_eq!(overview.allocations, 1);

    d.camera.zoom = 0.1;
    sync(&mut overview, &mut d, gpu);
    assert!(
        overview.tex_width < 2048,
        "zoomed out picks a coarser level"
    );
    assert_eq!(overview.allocations, 2);
    assert_eq!(resident_textures(&overview), 2);

    d.camera.zoom = 1.0;
    sync(&mut overview, &mut d, gpu);
    assert_eq!(overview.tex_width, 2048);
    assert_eq!(overview.allocations, 2, "the fine level is still there");
    gpu_matches_flatten(&overview, &d, gpu);
}

#[test]
fn warn_pressure_drops_levels_that_are_not_on_screen() {
    let Some(gpu) = gpu() else { return };
    let mut overview = pass(gpu);
    let mut d = doc(2048, 2048);
    overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false);
    d.camera.zoom = 1.0;
    sync(&mut overview, &mut d, gpu);
    d.camera.zoom = 0.1;
    sync(&mut overview, &mut d, gpu);
    assert_eq!(resident_textures(&overview), 2);

    let mut warn = GpuBudget::new(DeviceTier::Standard);
    warn.report_pressure(MemoryPressureLevel::Warn);
    overview.sync(&mut d, &gpu.device, &gpu.queue, &warn);

    assert_eq!(resident_textures(&overview), 1);
    assert!(has_texture(&overview));
    gpu_matches_flatten(&overview, &d, gpu);
}

#[test]
fn a_low_tier_budget_never_builds_a_level_past_its_cap() {
    let Some(gpu) = gpu() else { return };
    let mut overview = pass(gpu);
    let mut d = doc(4096, 4096);
    overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false);
    d.camera.zoom = 1.0;
    let low = GpuBudget::new(DeviceTier::Low);
    overview.sync(&mut d, &gpu.device, &gpu.queue, &low);
    assert_eq!(overview.tex_width, 2048);
    gpu_matches_flatten(&overview, &d, gpu);
}

#[test]
fn hiding_a_layer_rewrites_the_displayed_level_in_place() {
    let Some(gpu) = gpu() else { return };
    let mut overview = pass(gpu);
    let mut d = doc(256, 256);
    overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false);
    paint(&mut d, DocRect::new(8, 8, 40, 40), [200, 30, 40, 255]);
    sync(&mut overview, &mut d, gpu);
    gpu_matches_flatten(&overview, &d, gpu);

    d.layers[1].visible = false;
    sync(&mut overview, &mut d, gpu);
    assert_eq!(overview.allocations, 1);
    gpu_matches_flatten(&overview, &d, gpu);
}

#[test]
fn recoloring_a_vector_rebuilds_the_flatten_without_a_new_texture() {
    let Some(gpu) = gpu() else { return };
    let mut overview = pass(gpu);
    let mut d = doc(128, 128);
    d.add_vector_layer(
        "V",
        calumma_core::VectorItem::Shape(VectorShape {
            shape: Shape {
                tool: Tool::Rect,
                start: (16.0, 16.0),
                end: (96.0, 96.0),
                half_width: 1.0,
                fill: true,
                stroke: false,
            },
            color: [0, 90, 220, 255],
            stroke_color: [0, 90, 220, 255],
        }),
    );
    overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false);
    sync(&mut overview, &mut d, gpu);
    gpu_matches_flatten(&overview, &d, gpu);

    let Some(calumma_core::VectorItem::Shape(shape)) =
        d.layers.last_mut().unwrap().content.item_mut()
    else {
        panic!("vector layer");
    };
    shape.color = [220, 30, 30, 255];
    sync(&mut overview, &mut d, gpu);
    assert_eq!(overview.allocations, 1);
    gpu_matches_flatten(&overview, &d, gpu);
}

#[test]
fn a_paint_on_the_coarse_level_is_waiting_on_the_fine_one_after_zoom_in() {
    let Some(gpu) = gpu() else { return };
    let mut overview = pass(gpu);
    let mut d = doc(2048, 2048);
    overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false);
    d.camera.zoom = 1.0;
    d.camera.dpr = 1.0;
    sync(&mut overview, &mut d, gpu);
    d.camera.zoom = 0.1;
    sync(&mut overview, &mut d, gpu);
    assert_eq!(overview.allocations, 2);

    paint(&mut d, DocRect::new(16, 16, 80, 80), [20, 180, 80, 255]);
    sync(&mut overview, &mut d, gpu);
    gpu_matches_flatten(&overview, &d, gpu);

    d.camera.zoom = 1.0;
    sync(&mut overview, &mut d, gpu);
    assert_eq!(overview.tex_width, 2048);
    assert_eq!(overview.allocations, 2);
    gpu_matches_flatten(&overview, &d, gpu);
}

#[test]
fn a_higher_dpr_picks_a_finer_level_at_the_same_zoom() {
    let Some(gpu) = gpu() else { return };
    let mut overview = pass(gpu);
    let mut d = doc(2048, 2048);
    overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false);
    d.camera.zoom = 0.3;
    d.camera.dpr = 1.0;
    sync(&mut overview, &mut d, gpu);
    let at_one = overview.tex_width;

    d.camera.dpr = 2.0;
    sync(&mut overview, &mut d, gpu);
    assert!(
        overview.tex_width >= at_one,
        "dpr 2 must not pick a coarser flatten ({at_one} -> {})",
        overview.tex_width
    );
    gpu_matches_flatten(&overview, &d, gpu);
}

#[test]
fn critical_pressure_caps_the_finest_level_at_1024() {
    let Some(gpu) = gpu() else { return };
    let mut overview = pass(gpu);
    let mut d = doc(4096, 4096);
    overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false);
    d.camera.zoom = 1.0;
    let mut critical = GpuBudget::new(DeviceTier::Standard);
    critical.report_pressure(MemoryPressureLevel::Critical);
    overview.sync(&mut d, &gpu.device, &gpu.queue, &critical);
    assert_eq!(overview.tex_width, 1024);
    gpu_matches_flatten(&overview, &d, gpu);
}

#[test]
fn moving_a_layer_rewrites_the_flatten_to_match_the_cpu() {
    let Some(gpu) = gpu() else { return };
    let mut overview = pass(gpu);
    let mut d = doc(256, 256);
    overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false);
    paint(&mut d, DocRect::new(8, 8, 48, 48), [200, 30, 40, 255]);
    sync(&mut overview, &mut d, gpu);

    d.layers[1].transform = Some(LayerTransform {
        offset_x: 40.0,
        offset_y: 20.0,
        ..LayerTransform::default()
    });
    sync(&mut overview, &mut d, gpu);
    assert_eq!(overview.allocations, 1);
    gpu_matches_flatten(&overview, &d, gpu);
}

#[test]
fn an_adjustment_rewrites_the_flatten_to_match_the_cpu() {
    let Some(gpu) = gpu() else { return };
    let mut overview = pass(gpu);
    let mut d = doc(128, 128);
    overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false);
    paint(&mut d, DocRect::new(8, 8, 60, 60), [40, 80, 200, 255]);
    sync(&mut overview, &mut d, gpu);

    d.layers[1].adjustments = Some(Adjustments {
        brightness: 0.35,
        ..Adjustments::default()
    });
    sync(&mut overview, &mut d, gpu);
    assert_eq!(overview.allocations, 1);
    gpu_matches_flatten(&overview, &d, gpu);
}

#[test]
fn painting_two_chunks_keeps_the_gpu_in_lockstep_with_the_cpu() {
    let Some(gpu) = gpu() else { return };
    let mut overview = pass(gpu);
    let mut d = doc(2048, 512);
    overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false);
    d.camera.zoom = 1.0;
    sync(&mut overview, &mut d, gpu);
    paint(&mut d, DocRect::new(10, 10, 40, 40), [255, 0, 0, 255]);
    paint(&mut d, DocRect::new(1200, 10, 1240, 40), [0, 0, 255, 255]);
    sync(&mut overview, &mut d, gpu);
    assert_eq!(overview.allocations, 1);
    gpu_matches_flatten(&overview, &d, gpu);
}

#[test]
fn prewarm_then_an_active_sync_at_the_same_camera_does_not_allocate_again() {
    let Some(gpu) = gpu() else { return };
    let mut overview = pass(gpu);
    let mut d = doc(256, 256);
    d.camera.zoom = 1.0;
    overview.request_prewarm();
    overview.prewarm(&mut d, &gpu.device, &gpu.queue, &budget());
    assert_eq!(overview.allocations, 1);

    overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false);
    sync(&mut overview, &mut d, gpu);
    assert_eq!(overview.allocations, 1);
    gpu_matches_flatten(&overview, &d, gpu);
}
