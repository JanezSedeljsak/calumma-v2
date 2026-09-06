use calumma_core::smarttools::grabcut::{
    foreground_matte, foreground_matte_in_region, MatteParams,
};
use proptest::prelude::*;

fn blob_on_field(w: u32, h: u32, blob_w: u32, blob_h: u32, bg: [u8; 4], fg: [u8; 4]) -> Vec<u8> {
    let mut out = bg.repeat((w * h) as usize);
    let x0 = (w - blob_w) / 2;
    let y0 = (h - blob_h) / 2;
    for y in y0..y0 + blob_h {
        for x in x0..x0 + blob_w {
            let i = ((y * w + x) * 4) as usize;
            out[i..i + 4].copy_from_slice(&fg);
        }
    }
    out
}

fn at(mask: &[u8], w: u32, x: u32, y: u32) -> u8 {
    mask[(y * w + x) as usize]
}

fn drawn_ellipse(w: u32, h: u32, cx: f32, cy: f32, rx: f32, ry: f32) -> Vec<u8> {
    let mut out = vec![0u8; (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            let dx = (x as f32 + 0.5 - cx) / rx;
            let dy = (y as f32 + 0.5 - cy) / ry;
            if dx * dx + dy * dy <= 1.0 {
                out[(y * w + x) as usize] = 255;
            }
        }
    }
    out
}

#[test]
fn a_contrasting_blob_is_separated_from_its_background() {
    let (w, h) = (64u32, 64u32);
    let src = blob_on_field(w, h, 24, 24, [20, 40, 200, 255], [220, 30, 30, 255]);
    let mask = foreground_matte(&src, w, h, &MatteParams::default()).expect("a matte");
    assert_eq!(mask.len(), (w * h) as usize);
    assert_eq!(at(&mask, w, 32, 32), 255, "the middle of the blob is kept");
    assert_eq!(at(&mask, w, 30, 34), 255, "and so is the rest of it");
    assert_eq!(at(&mask, w, 2, 2), 0, "the corner of the field is cut away");
    assert_eq!(at(&mask, w, 32, 4), 0, "so is the field above the blob");
}

#[test]
fn already_transparent_pixels_are_never_kept() {
    let (w, h) = (48u32, 48u32);
    let mut src = blob_on_field(w, h, 20, 20, [0, 0, 0, 0], [200, 200, 40, 255]);
    for y in 22..26 {
        for x in 22..26 {
            let i = ((y * w + x) * 4) as usize;
            src[i..i + 4].copy_from_slice(&[200, 200, 40, 0]);
        }
    }
    let mask = foreground_matte(&src, w, h, &MatteParams::default()).expect("a matte");
    assert_eq!(at(&mask, w, 23, 23), 0, "the transparent hole stays cut");
    assert_eq!(
        at(&mask, w, 18, 24),
        255,
        "the opaque blob around it is kept"
    );
}

#[test]
fn a_fully_transparent_layer_has_no_matte() {
    let src = vec![0u8; 32 * 32 * 4];
    assert!(foreground_matte(&src, 32, 32, &MatteParams::default()).is_none());
}

#[test]
fn a_mismatched_buffer_length_is_refused_rather_than_panicking() {
    let src = vec![0u8; 10];
    assert!(foreground_matte(&src, 32, 32, &MatteParams::default()).is_none());
    assert!(foreground_matte(&[], 0, 0, &MatteParams::default()).is_none());
}

#[test]
fn a_layer_larger_than_the_work_resolution_still_mattes_at_full_size() {
    let (w, h) = (300u32, 200u32);
    let src = blob_on_field(w, h, 120, 90, [10, 10, 10, 255], [240, 240, 240, 255]);
    let params = MatteParams {
        max_side: 64,
        ..Default::default()
    };
    let mask = foreground_matte(&src, w, h, &params).expect("a matte");
    assert_eq!(mask.len(), (w * h) as usize);
    assert_eq!(
        at(&mask, w, 150, 100),
        255,
        "the blob's centre survives the round trip"
    );
    assert_eq!(at(&mask, w, 3, 3), 0, "and the corner is still background");
}

#[test]
fn the_matte_is_deterministic() {
    let (w, h) = (48u32, 48u32);
    let src = blob_on_field(w, h, 18, 18, [30, 90, 30, 255], [200, 60, 160, 255]);
    let a = foreground_matte(&src, w, h, &MatteParams::default()).expect("a matte");
    let b = foreground_matte(&src, w, h, &MatteParams::default()).expect("a matte");
    assert_eq!(a, b);
}

#[test]
fn the_smoothness_term_keeps_the_matte_from_shredding() {
    let (w, h) = (64u32, 64u32);
    let mut src = blob_on_field(w, h, 28, 28, [40, 40, 40, 255], [210, 210, 210, 255]);
    for y in 0..h {
        for x in 0..w {
            if (x * 7 + y * 13) % 11 == 0 {
                let i = ((y * w + x) * 4) as usize;
                let jitter = if src[i] > 128 { 40 } else { 215 };
                src[i] = jitter;
                src[i + 1] = jitter;
                src[i + 2] = jitter;
            }
        }
    }
    let mask = foreground_matte(&src, w, h, &MatteParams::default()).expect("a matte");
    let kept = mask.iter().filter(|&&m| m > 127).count();
    assert!(
        (400..1600).contains(&kept),
        "matte covers {kept} px, which is not the blob"
    );
}

#[test]
fn everything_outside_the_drawn_region_is_cut_away() {
    let (w, h) = (64u32, 64u32);
    let mut src = [20u8, 40, 200, 255].repeat((w * h) as usize);
    for y in 24..40 {
        for x in 8..24 {
            let i = ((y * w + x) * 4) as usize;
            src[i..i + 4].copy_from_slice(&[220, 30, 30, 255]);
        }
        for x in 40..56 {
            let i = ((y * w + x) * 4) as usize;
            src[i..i + 4].copy_from_slice(&[220, 30, 30, 255]);
        }
    }
    let region = drawn_ellipse(w, h, 16.0, 32.0, 14.0, 14.0);
    let mask =
        foreground_matte_in_region(&src, w, h, &region, &MatteParams::default()).expect("a matte");
    assert_eq!(
        at(&mask, w, 16, 32),
        255,
        "the square inside the loop is kept"
    );
    assert_eq!(
        at(&mask, w, 48, 32),
        0,
        "the identical square outside it is cut, colour notwithstanding"
    );
}

#[test]
fn background_inside_a_loose_region_is_still_cut_away() {
    let (w, h) = (64u32, 64u32);
    let src = blob_on_field(w, h, 16, 16, [20, 40, 200, 255], [220, 30, 30, 255]);
    let region = drawn_ellipse(w, h, 32.0, 32.0, 26.0, 26.0);
    let mask =
        foreground_matte_in_region(&src, w, h, &region, &MatteParams::default()).expect("a matte");
    assert_eq!(at(&mask, w, 32, 32), 255, "the blob itself is kept");
    assert_eq!(
        at(&mask, w, 32, 12),
        0,
        "field inside the loop but outside the blob is still cut"
    );
}

#[test]
fn a_drawn_region_rescues_a_subject_the_border_ring_would_have_condemned() {
    let (w, h) = (64u32, 64u32);
    let mut src = [20u8, 40, 200, 255].repeat((w * h) as usize);
    for y in 20..44 {
        for x in 0..28 {
            let i = ((y * w + x) * 4) as usize;
            src[i..i + 4].copy_from_slice(&[220, 30, 30, 255]);
        }
    }
    let auto = foreground_matte(&src, w, h, &MatteParams::default());
    assert!(
        auto.as_ref().is_none_or(|m| at(m, w, 1, 32) == 0),
        "auto seeding cannot keep a subject that runs through the rim"
    );
    let region = drawn_ellipse(w, h, 12.0, 32.0, 20.0, 16.0);
    let drawn =
        foreground_matte_in_region(&src, w, h, &region, &MatteParams::default()).expect("a matte");
    assert_eq!(
        at(&drawn, w, 1, 32),
        255,
        "drawing around it keeps the part that runs off the edge"
    );
}

#[test]
fn a_region_covering_everything_falls_back_to_the_border_ring() {
    let (w, h) = (48u32, 48u32);
    let src = blob_on_field(w, h, 16, 16, [30, 30, 30, 255], [230, 230, 230, 255]);
    let region = vec![255u8; (w * h) as usize];
    let mask =
        foreground_matte_in_region(&src, w, h, &region, &MatteParams::default()).expect("a matte");
    assert_eq!(at(&mask, w, 24, 24), 255, "the blob is still found");
    assert_eq!(
        at(&mask, w, 1, 1),
        0,
        "and the rim still reads as background"
    );
}

#[test]
fn a_region_of_the_wrong_size_falls_back_to_automatic_seeding() {
    let (w, h) = (48u32, 48u32);
    let src = blob_on_field(w, h, 18, 18, [30, 90, 30, 255], [200, 60, 160, 255]);
    let bogus = vec![255u8; 10];
    let drawn = foreground_matte_in_region(&src, w, h, &bogus, &MatteParams::default());
    let auto = foreground_matte(&src, w, h, &MatteParams::default());
    assert_eq!(drawn, auto);
}

#[test]
fn a_single_iteration_still_separates_a_contrasting_blob() {
    let (w, h) = (48u32, 48u32);
    let src = blob_on_field(w, h, 18, 18, [10, 20, 180, 255], [230, 40, 20, 255]);
    let params = MatteParams {
        iterations: 1,
        ..Default::default()
    };
    let mask = foreground_matte(&src, w, h, &params).expect("a matte");
    assert_eq!(at(&mask, w, 24, 24), 255);
    assert_eq!(at(&mask, w, 1, 1), 0);
}

#[test]
fn a_single_gmm_component_still_separates_two_flat_colours() {
    let (w, h) = (40u32, 40u32);
    let src = blob_on_field(w, h, 16, 16, [0, 0, 0, 255], [255, 255, 255, 255]);
    let region = drawn_ellipse(w, h, 20.0, 20.0, 12.0, 12.0);
    let params = MatteParams {
        components: 1,
        iterations: 2,
        ..Default::default()
    };
    let mask = foreground_matte_in_region(&src, w, h, &region, &params).expect("a matte");
    assert_eq!(at(&mask, w, 20, 20), 255);
    assert_eq!(at(&mask, w, 2, 2), 0);
}

#[test]
fn a_non_square_layer_mattes_at_its_own_size() {
    let (w, h) = (80u32, 40u32);
    let src = blob_on_field(w, h, 24, 16, [40, 10, 10, 255], [20, 200, 40, 255]);
    let mask = foreground_matte(&src, w, h, &MatteParams::default()).expect("a matte");
    assert_eq!(mask.len(), (w * h) as usize);
    assert_eq!(at(&mask, w, 40, 20), 255);
    assert_eq!(at(&mask, w, 2, 2), 0);
}

#[test]
fn a_tiny_layer_still_finds_the_blob() {
    let (w, h) = (16u32, 16u32);
    let src = blob_on_field(w, h, 6, 6, [0, 0, 255, 255], [255, 0, 0, 255]);
    let mask = foreground_matte(&src, w, h, &MatteParams::default()).expect("a matte");
    assert_eq!(at(&mask, w, 8, 8), 255);
    assert_eq!(at(&mask, w, 0, 0), 0);
}

#[test]
fn a_drawn_region_is_deterministic() {
    let (w, h) = (48u32, 48u32);
    let src = blob_on_field(w, h, 16, 16, [20, 40, 200, 255], [220, 30, 30, 255]);
    let region = drawn_ellipse(w, h, 24.0, 24.0, 18.0, 18.0);
    let a = foreground_matte_in_region(&src, w, h, &region, &MatteParams::default()).expect("a");
    let b = foreground_matte_in_region(&src, w, h, &region, &MatteParams::default()).expect("b");
    assert_eq!(a, b);
}

#[test]
fn isolated_speckle_is_not_kept_as_foreground() {
    let (w, h) = (48u32, 48u32);
    let mut src = blob_on_field(w, h, 18, 18, [20, 40, 200, 255], [220, 30, 30, 255]);
    let i = ((2 * w + 2) * 4) as usize;
    src[i..i + 4].copy_from_slice(&[220, 30, 30, 255]);
    let mask = foreground_matte(&src, w, h, &MatteParams::default()).expect("a matte");
    assert_eq!(
        at(&mask, w, 2, 2),
        0,
        "a 1px island on the field is dropped"
    );
    assert_eq!(at(&mask, w, 24, 24), 255);
}

#[test]
fn default_params_are_the_interactive_set() {
    let p = MatteParams::default();
    assert_eq!(p.max_side, 512);
    assert_eq!(p.components, 5);
    assert_eq!(p.iterations, 3);
}

#[test]
fn close_colours_still_separate_when_a_region_is_drawn() {
    let (w, h) = (48u32, 48u32);
    let src = blob_on_field(w, h, 18, 18, [40, 50, 90, 255], [90, 45, 40, 255]);
    let region = drawn_ellipse(w, h, 24.0, 24.0, 16.0, 16.0);
    let mask =
        foreground_matte_in_region(&src, w, h, &region, &MatteParams::default()).expect("a matte");
    assert_eq!(at(&mask, w, 24, 24), 255);
    assert_eq!(at(&mask, w, 2, 2), 0);
}

#[test]
fn a_downsampled_cut_comes_back_with_a_soft_edge() {
    let (w, h) = (160u32, 160u32);
    let src = blob_on_field(w, h, 64, 64, [10, 20, 200, 255], [230, 30, 20, 255]);
    let params = MatteParams {
        max_side: 40,
        ..Default::default()
    };
    let mask = foreground_matte(&src, w, h, &params).expect("a matte");
    assert_eq!(at(&mask, w, 80, 80), 255);
    assert_eq!(at(&mask, w, 2, 2), 0);
    let soft = mask.iter().filter(|&&m| m > 0 && m < 255).count();
    assert!(soft > 0, "the upsampled cut should feather the boundary");
}

#[test]
fn a_second_medium_blob_survives_if_it_is_inside_the_region() {
    let (w, h) = (64u32, 64u32);
    let mut src = [20u8, 40, 200, 255].repeat((w * h) as usize);
    for y in 20..36 {
        for x in 12..28 {
            let i = ((y * w + x) * 4) as usize;
            src[i..i + 4].copy_from_slice(&[220, 30, 30, 255]);
        }
        for x in 36..52 {
            let i = ((y * w + x) * 4) as usize;
            src[i..i + 4].copy_from_slice(&[220, 30, 30, 255]);
        }
    }
    let region = drawn_ellipse(w, h, 32.0, 28.0, 28.0, 18.0);
    let mask =
        foreground_matte_in_region(&src, w, h, &region, &MatteParams::default()).expect("a matte");
    assert_eq!(at(&mask, w, 20, 28), 255);
    assert_eq!(at(&mask, w, 44, 28), 255);
    assert_eq!(at(&mask, w, 2, 2), 0);
}

#[test]
fn a_thin_subject_is_not_eaten_by_cleanup() {
    let (w, h) = (48u32, 48u32);
    let mut src = [30u8, 30, 180, 255].repeat((w * h) as usize);
    for y in 10..38 {
        for x in 22..26 {
            let i = ((y * w + x) * 4) as usize;
            src[i..i + 4].copy_from_slice(&[240, 220, 40, 255]);
        }
    }
    let region = drawn_ellipse(w, h, 24.0, 24.0, 10.0, 18.0);
    let mask =
        foreground_matte_in_region(&src, w, h, &region, &MatteParams::default()).expect("a matte");
    assert_eq!(at(&mask, w, 24, 24), 255);
    assert_eq!(at(&mask, w, 24, 12), 255);
}

#[test]
fn a_subject_on_a_fully_transparent_field_is_kept() {
    let (w, h) = (48u32, 48u32);
    let src = blob_on_field(w, h, 18, 18, [0, 0, 0, 0], [200, 40, 30, 255]);
    let mask = foreground_matte(&src, w, h, &MatteParams::default()).expect("a matte");
    assert_eq!(mask.len(), (w * h) as usize);
    assert_eq!(at(&mask, w, 24, 24), 255);
    assert_eq!(at(&mask, w, 2, 2), 0);
}

#[test]
fn leftover_rgb_in_transparent_pixels_does_not_become_foreground() {
    let (w, h) = (48u32, 48u32);
    let src = blob_on_field(w, h, 16, 16, [220, 30, 30, 0], [40, 180, 40, 255]);
    let mask = foreground_matte(&src, w, h, &MatteParams::default()).expect("a matte");
    assert_eq!(at(&mask, w, 24, 24), 255);
    assert_eq!(at(&mask, w, 2, 2), 0);
}

#[test]
fn max_side_of_one_does_not_panic() {
    let (w, h) = (32u32, 24u32);
    let src = blob_on_field(w, h, 12, 12, [10, 20, 180, 255], [230, 40, 20, 255]);
    let params = MatteParams {
        max_side: 1,
        ..Default::default()
    };
    let out = foreground_matte(&src, w, h, &params);
    if let Some(mask) = out {
        assert_eq!(mask.len(), (w * h) as usize);
    }
}

#[test]
fn an_all_zero_region_has_no_matte() {
    let (w, h) = (32u32, 32u32);
    let src = blob_on_field(w, h, 12, 12, [20, 40, 200, 255], [220, 30, 30, 255]);
    let region = vec![0u8; (w * h) as usize];
    assert!(foreground_matte_in_region(&src, w, h, &region, &MatteParams::default()).is_none());
}

#[test]
fn a_circular_blob_is_kept_as_a_disk() {
    let (w, h) = (64u32, 64u32);
    let mut src = [20u8, 40, 200, 255].repeat((w * h) as usize);
    let (cx, cy, r) = (32.0f32, 32.0f32, 14.0f32);
    for y in 0..h {
        for x in 0..w {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            if dx * dx + dy * dy <= r * r {
                let i = ((y * w + x) * 4) as usize;
                src[i..i + 4].copy_from_slice(&[220, 30, 30, 255]);
            }
        }
    }
    let mask = foreground_matte(&src, w, h, &MatteParams::default()).expect("a matte");
    assert_eq!(at(&mask, w, 32, 32), 255);
    assert_eq!(at(&mask, w, 32, 32 - 10), 255);
    assert_eq!(at(&mask, w, 2, 2), 0);
    assert_eq!(at(&mask, w, 32, 2), 0);
}

#[test]
fn a_drawn_region_survives_work_resolution_shrink() {
    let (w, h) = (200u32, 160u32);
    let mut src = [20u8, 40, 200, 255].repeat((w * h) as usize);
    for y in 70..90 {
        for x in 20..50 {
            let i = ((y * w + x) * 4) as usize;
            src[i..i + 4].copy_from_slice(&[220, 30, 30, 255]);
        }
        for x in 150..180 {
            let i = ((y * w + x) * 4) as usize;
            src[i..i + 4].copy_from_slice(&[220, 30, 30, 255]);
        }
    }
    let region = drawn_ellipse(w, h, 35.0, 80.0, 28.0, 22.0);
    let params = MatteParams {
        max_side: 48,
        ..Default::default()
    };
    let mask = foreground_matte_in_region(&src, w, h, &region, &params).expect("a matte");
    assert_eq!(mask.len(), (w * h) as usize);
    assert!(
        at(&mask, w, 35, 80) > 127,
        "the looped square survives the shrink"
    );
    assert_eq!(
        at(&mask, w, 165, 80),
        0,
        "the twin outside the loop is still cut"
    );
}

proptest! {
    #[test]
    fn a_matte_is_either_absent_or_the_full_grid(
        w in 2u32..16,
        h in 2u32..16,
        seed in 0u64..4_000,
        max_side in 1u32..32,
    ) {
        let mut src = vec![0u8; (w * h * 4) as usize];
        let mut x = seed;
        for px in src.chunks_exact_mut(4) {
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1);
            px[0] = (x >> 8) as u8;
            px[1] = (x >> 16) as u8;
            px[2] = (x >> 24) as u8;
            px[3] = if x & 7 == 0 { 0 } else { 255 };
        }
        let params = MatteParams {
            max_side,
            iterations: 1,
            components: 2,
            ..Default::default()
        };
        match foreground_matte(&src, w, h, &params) {
            None => {}
            Some(mask) => prop_assert_eq!(mask.len(), (w * h) as usize),
        }
    }
}
