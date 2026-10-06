use super::*;
use crate::test_gpu::{gpu, read_texture_layer, Gpu};

const SIDE: u32 = 64;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8UnormSrgb;
const SCISSOR: PxRect = (8, 8, 32, 32);

fn sized(gpu: &Gpu) -> PanCache {
    let mut cache = PanCache::new(&gpu.device, &gpu.shader, FORMAT);
    cache.resize(&gpu.device, SIDE, SIDE);
    cache
}

/// A reference-holding cache at pan `(0, 0)`, zoom 1, dpr 1 — the state every frame after
/// a full redraw starts from.
fn referenced(gpu: &Gpu) -> PanCache {
    let mut cache = sized(gpu);
    cache.commit_reference((0.0, 0.0), 1.0, 1.0, SCISSOR);
    cache
}

fn fill(gpu: &Gpu, texture: &wgpu::Texture, value: u8) {
    let pixels = vec![value; (SIDE * SIDE * 4) as usize];
    gpu.queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(SIDE * 4),
            rows_per_image: Some(SIDE),
        },
        wgpu::Extent3d {
            width: SIDE,
            height: SIDE,
            depth_or_array_layers: 1,
        },
    );
}

#[test]
fn a_cache_with_no_reference_yet_matches_nothing_and_plans_nothing() {
    let Some(gpu) = gpu() else { return };
    let cache = sized(gpu);

    assert!(!cache.reference_matches((0.0, 0.0), 1.0, 1.0, SCISSOR));
    assert!(cache.plan((0.0, 0.0), 1.0, 1.0, SCISSOR).is_none());
}

#[test]
fn an_unmoved_camera_matches_the_reference_so_the_frame_can_skip_the_content_pass() {
    let Some(gpu) = gpu() else { return };
    let cache = referenced(gpu);

    assert!(cache.reference_matches((0.0, 0.0), 1.0, 1.0, SCISSOR));
}

/// The match test is "no whole *device* pixel of movement", not float equality: a blit can
/// only express whole pixels, so a sub-pixel drift is content the reference already holds.
/// Demanding equality would send every post-blit frame down the blit path to copy zero
/// distance.
#[test]
fn a_sub_pixel_camera_drift_still_counts_as_a_match() {
    let Some(gpu) = gpu() else { return };
    let cache = referenced(gpu);

    assert!(cache.reference_matches((0.4, -0.4), 1.0, 1.0, SCISSOR));
    assert!(!cache.reference_matches((1.0, 0.0), 1.0, 1.0, SCISSOR));
}

#[test]
fn dpr_decides_how_far_the_camera_may_drift_before_it_is_a_whole_pixel() {
    let Some(gpu) = gpu() else { return };
    let mut cache = sized(gpu);
    cache.commit_reference((0.0, 0.0), 1.0, 2.0, SCISSOR);

    assert!(
        !cache.reference_matches((0.3, 0.0), 1.0, 2.0, SCISSOR),
        "0.3 logical points is 0.6 device pixels, which rounds to a whole one"
    );
    assert!(cache.reference_matches((0.2, 0.0), 1.0, 2.0, SCISSOR));
}

#[test]
fn zoom_dpr_and_scissor_changes_all_break_the_match() {
    let Some(gpu) = gpu() else { return };
    let cache = referenced(gpu);

    assert!(!cache.reference_matches((0.0, 0.0), 2.0, 1.0, SCISSOR));
    assert!(!cache.reference_matches((0.0, 0.0), 1.0, 2.0, SCISSOR));
    assert!(!cache.reference_matches((0.0, 0.0), 1.0, 1.0, (0, 0, 32, 32)));
}

#[test]
fn a_pan_plans_a_blit_of_the_overlap_with_the_rounded_shift() {
    let Some(gpu) = gpu() else { return };
    let cache = referenced(gpu);

    let plan = cache
        .plan((4.0, 0.0), 1.0, 1.0, SCISSOR)
        .expect("an overlapping pan is blittable");

    assert_eq!(plan.shift, (4, 0));
    assert_eq!(plan.dst.2, plan.src.2);
    assert_eq!(plan.dst.3, plan.src.3);
    assert_eq!(plan.dst.0, plan.src.0 + 4);
    assert!(plan.src.2 < SCISSOR.2, "the leading edge is not covered");
}

#[test]
fn a_zoom_change_refuses_a_blit_because_the_pixels_are_a_different_size() {
    let Some(gpu) = gpu() else { return };
    let cache = referenced(gpu);

    assert!(cache.plan((0.0, 0.0), 1.5, 1.0, SCISSOR).is_none());
    assert!(cache.plan((0.0, 0.0), 1.0, 2.0, SCISSOR).is_none());
}

#[test]
fn a_pan_past_the_whole_scissor_leaves_no_overlap_to_copy() {
    let Some(gpu) = gpu() else { return };
    let cache = referenced(gpu);

    assert!(cache.plan((1000.0, 0.0), 1.0, 1.0, SCISSOR).is_none());
}

#[test]
fn invalidating_forces_the_next_frame_through_a_full_redraw() {
    let Some(gpu) = gpu() else { return };
    let mut cache = referenced(gpu);

    cache.invalidate();

    assert!(!cache.reference_matches((0.0, 0.0), 1.0, 1.0, SCISSOR));
    assert!(cache.plan((4.0, 0.0), 1.0, 1.0, SCISSOR).is_none());
}

/// Chaining frame to frame is only safe because the reference pan advances by the shift
/// that was actually blitted, in whole device pixels. Ten pans of half a pixel each must
/// therefore land on exactly five pixels of movement, with nothing accumulated.
#[test]
fn committing_a_shift_advances_the_reference_by_exactly_what_was_blitted() {
    let Some(gpu) = gpu() else { return };
    let mut cache = referenced(gpu);
    let mut pan = 0.0f32;

    for _ in 0..10 {
        pan += 0.5;
        if let Some(plan) = cache.plan((pan, 0.0), 1.0, 1.0, SCISSOR) {
            cache.commit_shift(plan.shift, 1.0, SCISSOR);
        }
    }

    assert_eq!(cache.reference_pan, (5.0, 0.0));
    assert!(cache.reference_matches((5.0, 0.0), 1.0, 1.0, SCISSOR));
}

#[test]
fn committing_a_shift_at_a_dpr_of_two_moves_the_reference_half_as_far() {
    let Some(gpu) = gpu() else { return };
    let mut cache = sized(gpu);
    cache.commit_reference((0.0, 0.0), 1.0, 2.0, SCISSOR);

    cache.commit_shift((5, -3), 2.0, SCISSOR);

    assert_eq!(cache.reference_pan, (2.5, -1.5));
}

/// The board pass always samples `reference`, so the texture a shift just drew into has to
/// become the reference — otherwise a pan would show the frame before last.
#[test]
fn committing_a_shift_promotes_the_working_texture_to_be_the_reference() {
    let Some(gpu) = gpu() else { return };
    let mut cache = referenced(gpu);
    fill(gpu, cache.reference_texture(), 0x11);
    fill(gpu, cache.working_texture(), 0x99);

    cache.commit_shift((1, 0), 1.0, SCISSOR);

    let read = read_texture_layer(&gpu.device, &gpu.queue, cache.reference_texture(), 0, SIDE);
    assert!(read.iter().all(|&b| b == 0x99));
}

#[test]
fn resizing_to_the_same_size_keeps_the_reference_and_a_real_resize_drops_it() {
    let Some(gpu) = gpu() else { return };
    let mut cache = referenced(gpu);

    cache.resize(&gpu.device, SIDE, SIDE);
    assert!(
        cache.reference_matches((0.0, 0.0), 1.0, 1.0, SCISSOR),
        "a no-op resize must not throw the cached frame away"
    );

    cache.resize(&gpu.device, SIDE * 2, SIDE);
    assert!(!cache.reference_matches((0.0, 0.0), 1.0, 1.0, SCISSOR));
    assert_eq!(cache.reference_texture().width(), SIDE * 2);
}
