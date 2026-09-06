use calumma_core::smarttools::seam_carving::{energy_map, seam_carve};
use proptest::prelude::*;

fn solid(w: u32, h: u32, rgba: [u8; 4]) -> Vec<u8> {
    rgba.repeat((w * h) as usize)
}

#[test]
fn a_flat_field_has_zero_energy_everywhere() {
    let src = solid(6, 6, [80, 80, 80, 255]);
    let energy = energy_map(&src, 6, 6);
    assert!(energy.iter().all(|&e| e.abs() < 1e-4));
}

#[test]
fn a_vertical_edge_has_high_energy_along_the_edge_and_none_elsewhere() {
    let (w, h) = (8u32, 4u32);
    let mut src = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            let v = if x < 4 { 0 } else { 255 };
            src[i..i + 4].copy_from_slice(&[v, v, v, 255]);
        }
    }
    let energy = energy_map(&src, w, h);
    for y in 0..h as usize {
        assert!(energy[y * w as usize + 3] > 50.0, "left of the edge");
        assert!(energy[y * w as usize + 4] > 50.0, "right of the edge");
        assert!(energy[y * w as usize] < 1e-4, "flat, far from the edge");
    }
}

#[test]
fn a_transparency_boundary_carries_energy_even_with_flat_color() {
    let (w, h) = (8u32, 2u32);
    let mut src = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            let a = if x < 4 { 0 } else { 255 };
            src[i..i + 4].copy_from_slice(&[10, 10, 10, a]);
        }
    }
    let energy = energy_map(&src, w, h);
    assert!(energy[3] > 50.0);
    assert!(energy[4] > 50.0);
}

#[test]
fn removing_one_seam_from_a_flat_field_narrows_it_by_one() {
    let src = solid(10, 5, [1, 2, 3, 255]);
    let out = seam_carve(&src, 10, 5, 9, 5);
    assert_eq!(out.len(), 9 * 5 * 4);
    assert!(out.chunks_exact(4).all(|p| p == [1, 2, 3, 255]));
}

#[test]
fn inserting_a_seam_widens_a_flat_field_by_one() {
    let src = solid(10, 5, [9, 8, 7, 255]);
    let out = seam_carve(&src, 10, 5, 11, 5);
    assert_eq!(out.len(), 11 * 5 * 4);
    assert!(out.chunks_exact(4).all(|p| p == [9, 8, 7, 255]));
}

#[test]
fn carving_both_axes_reaches_the_exact_target_size() {
    let src = solid(12, 8, [5, 5, 5, 255]);
    let out = seam_carve(&src, 12, 8, 8, 6);
    assert_eq!(out.len(), 8 * 6 * 4);
}

#[test]
fn an_unchanged_target_is_a_plain_copy() {
    let src: Vec<u8> = (0..(6 * 6 * 4)).map(|i| (i % 251) as u8).collect();
    assert_eq!(seam_carve(&src, 6, 6, 6, 6), src);
}

#[test]
fn carving_width_preserves_a_high_energy_subject() {
    let (w, h) = (20u32, 6u32);
    let mut src = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            let v = if (9..12).contains(&x) {
                if (x + y) % 2 == 0 {
                    255
                } else {
                    0
                }
            } else {
                128
            };
            src[i..i + 4].copy_from_slice(&[v, v, v, 255]);
        }
    }
    let out = seam_carve(&src, w, h, 12, h);
    for y in 0..h as usize {
        let row = &out[y * 12 * 4..(y + 1) * 12 * 4];
        let vals: Vec<u8> = row.chunks_exact(4).map(|p| p[0]).collect();
        let high = vals.iter().any(|&v| v > 200);
        let low = vals.iter().any(|&v| v < 55);
        assert!(
            high && low,
            "row {y} lost the high-contrast subject: {vals:?}"
        );
    }
}

#[test]
fn carving_height_only_does_not_change_width() {
    let src = solid(10, 8, [4, 5, 6, 255]);
    let out = seam_carve(&src, 10, 8, 10, 6);
    assert_eq!(out.len(), 10 * 6 * 4);
    assert!(out.chunks_exact(4).all(|p| p == [4, 5, 6, 255]));
}

#[test]
fn carving_width_only_does_not_change_height() {
    let src = solid(10, 8, [4, 5, 6, 255]);
    let out = seam_carve(&src, 10, 8, 7, 8);
    assert_eq!(out.len(), 7 * 8 * 4);
}

#[test]
fn a_one_by_one_source_can_grow() {
    let src = [10u8, 20, 30, 255].to_vec();
    let out = seam_carve(&src, 1, 1, 3, 2);
    assert_eq!(out.len(), 3 * 2 * 4);
    assert!(out.chunks_exact(4).all(|p| p == [10, 20, 30, 255]));
}

#[test]
fn zero_sized_targets_clamp_to_one() {
    let src = solid(4, 4, [1, 2, 3, 255]);
    let out = seam_carve(&src, 4, 4, 0, 0);
    assert_eq!(out.len(), 4);
}

#[test]
fn inserting_several_seams_into_a_flat_field_stays_flat() {
    let src = solid(6, 4, [50, 60, 70, 255]);
    let out = seam_carve(&src, 6, 4, 12, 4);
    assert_eq!(out.len(), 12 * 4 * 4);
    assert!(out.chunks_exact(4).all(|p| p == [50, 60, 70, 255]));
}

#[test]
fn carving_height_preserves_a_horizontal_subject() {
    let (w, h) = (6u32, 16u32);
    let mut src = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            let v = if (7..10).contains(&y) {
                if (x + y) % 2 == 0 {
                    255
                } else {
                    0
                }
            } else {
                128
            };
            src[i..i + 4].copy_from_slice(&[v, v, v, 255]);
        }
    }
    let out = seam_carve(&src, w, h, w, 10);
    let mut high = false;
    let mut low = false;
    for px in out.chunks_exact(4) {
        high |= px[0] > 200;
        low |= px[0] < 55;
    }
    assert!(high && low, "the high-contrast band was carved away");
}

#[test]
fn inserting_into_a_cheap_column_does_not_keep_duplicating_it() {
    let (w, h) = (8u32, 6u32);
    let mut src = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            let v = if x == 0 { 40 } else { 80 + (x as u8) * 8 };
            src[i..i + 4].copy_from_slice(&[v, v, v, 255]);
        }
    }
    let out = seam_carve(&src, w, h, 12, h);
    for y in 0..h as usize {
        let row: Vec<u8> = out[y * 12 * 4..(y + 1) * 12 * 4]
            .chunks_exact(4)
            .map(|p| p[0])
            .collect();
        let run = row.windows(5).any(|w| w.iter().all(|&v| v == 40));
        assert!(
            !run,
            "row {y} duplicated the cheap column into a stripe: {row:?}"
        );
    }
}

#[test]
fn shrinking_then_growing_reaches_the_later_size() {
    let src = solid(10, 6, [8, 8, 8, 255]);
    let mid = seam_carve(&src, 10, 6, 7, 6);
    let out = seam_carve(&mid, 7, 6, 11, 8);
    assert_eq!(out.len(), 11 * 8 * 4);
}

proptest! {
    #[test]
    fn output_byte_count_matches_the_target_grid(
        w in 2u32..10,
        h in 2u32..10,
        dw in 1u32..12,
        dh in 1u32..12,
    ) {
        let src = solid(w, h, [40, 40, 40, 255]);
        let out = seam_carve(&src, w, h, dw, dh);
        prop_assert_eq!(out.len(), (dw * dh * 4) as usize);
    }
}

#[test]
fn inserting_into_a_gradient_blends_the_two_neighbours() {
    let (w, h) = (6u32, 4u32);
    let mut src = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            let v = (x * 40) as u8;
            src[i..i + 4].copy_from_slice(&[v, v, v, 255]);
        }
    }
    let out = seam_carve(&src, w, h, w + 1, h);
    for y in 0..h as usize {
        let row: Vec<u8> = out[y * 7 * 4..(y + 1) * 7 * 4]
            .chunks_exact(4)
            .map(|p| p[0])
            .collect();
        for pair in row.windows(2) {
            assert!(
                pair[1] + 8 >= pair[0],
                "row {y} is no longer a gentle ramp: {row:?}"
            );
        }
        assert_eq!(*row.first().unwrap(), 0);
        assert_eq!(*row.last().unwrap(), 200);
    }
}

#[test]
fn a_two_by_two_field_can_collapse_to_one_pixel() {
    let src = solid(2, 2, [9, 8, 7, 255]);
    let out = seam_carve(&src, 2, 2, 1, 1);
    assert_eq!(out, [9, 8, 7, 255]);
}

#[test]
fn energy_of_a_single_pixel_is_zero() {
    let src = [10u8, 20, 30, 255].to_vec();
    let energy = energy_map(&src, 1, 1);
    assert_eq!(energy.len(), 1);
    assert!(energy[0].abs() < 1e-4);
}

#[test]
fn carving_is_deterministic() {
    let mut src = vec![0u8; 12 * 8 * 4];
    for i in 0..12 * 8 {
        let v = ((i * 13) % 180) as u8;
        src[i * 4..i * 4 + 4].copy_from_slice(&[v, v, 40, 255]);
    }
    let a = seam_carve(&src, 12, 8, 9, 6);
    let b = seam_carve(&src, 12, 8, 9, 6);
    assert_eq!(a, b);
}

fn isoluminant_red_green(w: u32, h: u32) -> Vec<u8> {
    let mut src = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            if x < w / 2 {
                src[i..i + 4].copy_from_slice(&[255, 0, 0, 255]);
            } else {
                src[i..i + 4].copy_from_slice(&[0, 130, 0, 255]);
            }
        }
    }
    src
}

#[test]
fn an_isoluminant_colour_edge_still_carries_energy() {
    let (w, h) = (8u32, 4u32);
    let src = isoluminant_red_green(w, h);
    let energy = energy_map(&src, w, h);
    for y in 0..h as usize {
        assert!(energy[y * w as usize + 3] > 50.0, "left of the chroma edge");
        assert!(
            energy[y * w as usize + 4] > 50.0,
            "right of the chroma edge"
        );
        assert!(energy[y * w as usize] < 1e-4, "flat, far from the edge");
    }
}

#[test]
fn narrowing_keeps_both_sides_of_an_isoluminant_split() {
    let (w, h) = (16u32, 6u32);
    let src = isoluminant_red_green(w, h);
    let out = seam_carve(&src, w, h, 10, h);
    let mut red = false;
    let mut green = false;
    for px in out.chunks_exact(4) {
        red |= px[0] > 200 && px[1] < 40;
        green |= px[1] > 80 && px[0] < 40;
    }
    assert!(
        red && green,
        "an isoluminant edge must not vanish under a carve"
    );
}

#[test]
fn energy_map_length_matches_the_pixel_grid() {
    let src = solid(5, 3, [10, 20, 30, 255]);
    assert_eq!(energy_map(&src, 5, 3).len(), 15);
}
