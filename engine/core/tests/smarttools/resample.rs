use calumma_core::smarttools::resample::{box_downsample, lanczos3_resize};
use proptest::prelude::*;

fn solid(w: u32, h: u32, rgba: [u8; 4]) -> Vec<u8> {
    rgba.repeat((w * h) as usize)
}

#[test]
fn a_solid_color_upscales_to_the_same_solid_color() {
    let src = solid(4, 4, [200, 40, 10, 255]);
    let out = lanczos3_resize(&src, 4, 4, 16, 16);
    assert_eq!(out.len(), 16 * 16 * 4);
    assert!(
        out.chunks_exact(4).all(|p| p == [200, 40, 10, 255]),
        "a flat field must stay flat under resampling, ringing included"
    );
}

#[test]
fn same_size_is_a_plain_copy() {
    let src: Vec<u8> = (0..(6 * 6 * 4)).map(|i| (i % 251) as u8).collect();
    assert_eq!(lanczos3_resize(&src, 6, 6, 6, 6), src);
}

#[test]
fn upscaling_doubles_the_pixel_grid() {
    let src = solid(3, 5, [10, 20, 30, 255]);
    let out = lanczos3_resize(&src, 3, 5, 6, 10);
    assert_eq!(out.len(), 6 * 10 * 4);
}

#[test]
fn downscaling_a_checkerboard_averages_rather_than_aliases() {
    let side = 32u32;
    let mut src = vec![0u8; (side * side * 4) as usize];
    for y in 0..side {
        for x in 0..side {
            let i = ((y * side + x) * 4) as usize;
            let v = if (x + y) % 2 == 0 { 255 } else { 0 };
            src[i..i + 4].copy_from_slice(&[v, v, v, 255]);
        }
    }
    let out = lanczos3_resize(&src, side, side, 4, 4);
    for px in out.chunks_exact(4) {
        assert!(
            (60..=195).contains(&px[0]),
            "checkerboard pixels should have averaged toward mid-gray, got {}",
            px[0]
        );
    }
}

#[test]
fn transparent_pixels_never_bleed_color_into_opaque_neighbours() {
    let mut src = solid(8, 1, [0, 0, 0, 0]);
    for i in 4..8 {
        src[i * 4..i * 4 + 4].copy_from_slice(&[255, 255, 255, 255]);
    }
    let out = lanczos3_resize(&src, 8, 1, 32, 1);
    for px in out[28 * 4..].chunks_exact(4) {
        assert_eq!(px, [255, 255, 255, 255]);
    }
}

#[test]
fn zero_sized_targets_do_not_panic() {
    let src = solid(2, 2, [1, 2, 3, 4]);
    let out = lanczos3_resize(&src, 2, 2, 0, 0);
    assert_eq!(out.len(), 4);
}

#[test]
fn width_only_scale_keeps_height() {
    let src = solid(8, 4, [12, 34, 56, 255]);
    let out = lanczos3_resize(&src, 8, 4, 16, 4);
    assert_eq!(out.len(), 16 * 4 * 4);
    assert!(out.chunks_exact(4).all(|p| p == [12, 34, 56, 255]));
}

#[test]
fn height_only_scale_keeps_width() {
    let src = solid(4, 8, [9, 8, 7, 255]);
    let out = lanczos3_resize(&src, 4, 8, 4, 16);
    assert_eq!(out.len(), 4 * 16 * 4);
    assert!(out.chunks_exact(4).all(|p| p == [9, 8, 7, 255]));
}

#[test]
fn a_one_pixel_source_upscales_to_a_flat_field() {
    let src = [40u8, 80, 120, 255].to_vec();
    let out = lanczos3_resize(&src, 1, 1, 8, 8);
    assert_eq!(out.len(), 8 * 8 * 4);
    assert!(out.chunks_exact(4).all(|p| p == [40, 80, 120, 255]));
}

#[test]
fn a_solid_half_alpha_stays_half_alpha() {
    let src = solid(6, 6, [20, 40, 80, 128]);
    let out = lanczos3_resize(&src, 6, 6, 12, 12);
    for px in out.chunks_exact(4) {
        assert!(
            (px[3] as i16 - 128).abs() <= 1,
            "alpha drifted to {}",
            px[3]
        );
        assert!((px[0] as i16 - 20).abs() <= 1);
    }
}

#[test]
fn a_fully_transparent_buffer_stays_transparent() {
    let src = solid(5, 5, [90, 10, 10, 0]);
    let out = lanczos3_resize(&src, 5, 5, 15, 9);
    assert!(out.chunks_exact(4).all(|p| p == [0, 0, 0, 0]));
}

#[test]
fn odd_sizes_round_trip_to_the_requested_grid() {
    let src = solid(5, 7, [1, 2, 3, 255]);
    let out = lanczos3_resize(&src, 5, 7, 9, 11);
    assert_eq!(out.len(), 9 * 11 * 4);
}

#[test]
fn downscaling_a_solid_stays_solid() {
    let src = solid(16, 10, [70, 80, 90, 255]);
    let out = lanczos3_resize(&src, 16, 10, 5, 3);
    assert_eq!(out.len(), 5 * 3 * 4);
    assert!(out.chunks_exact(4).all(|p| p == [70, 80, 90, 255]));
}

#[test]
fn a_wide_strip_resizes_without_changing_the_flat_colour() {
    let src = solid(64, 1, [255, 0, 128, 255]);
    let out = lanczos3_resize(&src, 64, 1, 8, 1);
    assert!(out.chunks_exact(4).all(|p| p == [255, 0, 128, 255]));
}

proptest! {
    #[test]
    fn output_byte_count_matches_the_target_grid(
        src_w in 1u32..24,
        src_h in 1u32..24,
        dst_w in 0u32..32,
        dst_h in 0u32..32,
    ) {
        let src = solid(src_w, src_h, [30, 60, 90, 255]);
        let out = lanczos3_resize(&src, src_w, src_h, dst_w, dst_h);
        let expect_w = dst_w.max(1);
        let expect_h = dst_h.max(1);
        prop_assert_eq!(out.len(), (expect_w * expect_h * 4) as usize);
    }
}

#[test]
fn a_two_colour_step_stays_flat_far_from_the_edge() {
    let mut src = solid(16, 4, [0, 0, 0, 255]);
    for y in 0..4u32 {
        for x in 8..16u32 {
            let i = ((y * 16 + x) * 4) as usize;
            src[i..i + 4].copy_from_slice(&[200, 40, 10, 255]);
        }
    }
    let out = lanczos3_resize(&src, 16, 4, 64, 4);
    for px in out.chunks_exact(4).take(8) {
        assert_eq!(px, [0, 0, 0, 255]);
    }
    let last = &out[(64 * 4 - 8) * 4..];
    for px in last.chunks_exact(4) {
        assert_eq!(px, [200, 40, 10, 255]);
    }
}

#[test]
fn box_downsample_averages_a_solid_to_the_same_solid() {
    let src = solid(12, 8, [15, 25, 35, 255]);
    let out = box_downsample(&src, 12, 8, 3, 2);
    assert_eq!(out.len(), 3 * 2 * 4);
    assert!(out.chunks_exact(4).all(|p| p == [15, 25, 35, 255]));
}

#[test]
fn box_downsample_averages_a_checkerboard_toward_mid_gray() {
    let side = 8u32;
    let mut src = vec![0u8; (side * side * 4) as usize];
    for y in 0..side {
        for x in 0..side {
            let i = ((y * side + x) * 4) as usize;
            let v = if (x + y) % 2 == 0 { 255 } else { 0 };
            src[i..i + 4].copy_from_slice(&[v, v, v, 255]);
        }
    }
    let out = box_downsample(&src, side, side, 2, 2);
    for px in out.chunks_exact(4) {
        assert!(
            (100..=155).contains(&px[0]),
            "expected a box-averaged mid-gray, got {}",
            px[0]
        );
        assert_eq!(px[3], 255);
    }
}

#[test]
fn box_downsample_same_size_is_a_copy() {
    let src: Vec<u8> = (0..(5 * 5 * 4)).map(|i| (i % 200) as u8).collect();
    assert_eq!(box_downsample(&src, 5, 5, 5, 5), src);
}

#[test]
fn box_downsample_does_not_let_transparent_colour_tint_opaque_neighbours() {
    let src = [255u8, 0, 0, 0, 0, 255, 0, 255].to_vec();
    let out = box_downsample(&src, 2, 1, 1, 1);
    assert_eq!(out.len(), 4);
    assert_eq!(out[3], 128, "alpha is the box average");
    assert!(
        out[1] > out[0] + 80,
        "the opaque green must dominate, got {out:?}"
    );
    assert!(out[0] < 40, "transparent red must not leak, got {out:?}");
}

#[test]
fn box_downsample_zero_sized_targets_do_not_panic() {
    let src = solid(2, 2, [1, 2, 3, 255]);
    let out = box_downsample(&src, 2, 2, 0, 0);
    assert_eq!(out.len(), 4);
}

#[test]
fn a_one_pixel_box_downsample_stays_that_pixel() {
    let src = [9u8, 8, 7, 255].to_vec();
    assert_eq!(
        box_downsample(&src, 1, 1, 4, 3),
        solid(4, 3, [9, 8, 7, 255])
    );
}
