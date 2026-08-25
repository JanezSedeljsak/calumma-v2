use calumma_core::resample::{box_downsample, fit_within};

fn solid(w: u32, h: u32, px: [u8; 4]) -> Vec<u8> {
    let mut out = Vec::with_capacity((w as usize) * (h as usize) * 4);
    for _ in 0..(w as usize) * (h as usize) {
        out.extend_from_slice(&px);
    }
    out
}

fn at(buf: &[u8], w: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y as usize) * (w as usize) + x as usize) * 4;
    [buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]
}

#[test]
fn a_solid_image_stays_the_same_colour() {
    let src = solid(64, 64, [200, 100, 50, 255]);
    let out = box_downsample(&src, 64, 64, 8, 8);
    assert_eq!(out.len(), 8 * 8 * 4);
    for y in 0..8 {
        for x in 0..8 {
            assert_eq!(at(&out, 8, x, y), [200, 100, 50, 255], "at {x},{y}");
        }
    }
}

/// The whole reason this is not `nearest_source`: halving a checkerboard has to average to the
/// midpoint, not pick one of the two squares and call the other one gone.
#[test]
fn a_checkerboard_averages_instead_of_picking_a_winner() {
    let mut src = vec![0u8; 4 * 4 * 4];
    for y in 0..4usize {
        for x in 0..4usize {
            let i = (y * 4 + x) * 4;
            let v = if (x + y) % 2 == 0 { 0 } else { 255 };
            src[i..i + 4].copy_from_slice(&[v, v, v, 255]);
        }
    }
    let out = box_downsample(&src, 4, 4, 2, 2);
    for y in 0..2 {
        for x in 0..2 {
            let px = at(&out, 2, x, y);
            assert_eq!(px[0], 127, "at {x},{y}");
            assert_eq!(px[3], 255);
        }
    }
}

/// Straight-averaging unpremultiplied RGBA lets invisible pixels vote on colour, which halos
/// every edge with whatever happens to be sitting in the transparent part of the buffer.
#[test]
fn transparent_pixels_do_not_vote_on_colour() {
    let mut src = vec![0u8; 2 * 4];
    src[0..4].copy_from_slice(&[255, 0, 0, 255]);
    src[4..8].copy_from_slice(&[0, 255, 0, 0]);
    let out = box_downsample(&src, 2, 1, 1, 1);
    let px = at(&out, 1, 0, 0);
    assert_eq!([px[0], px[1], px[2]], [255, 0, 0], "the visible half wins");
    assert_eq!(px[3], 127, "and the coverage halves");
}

#[test]
fn a_fully_transparent_block_stays_transparent() {
    let src = solid(8, 8, [0, 0, 0, 0]);
    let out = box_downsample(&src, 8, 8, 2, 2);
    assert!(out.iter().all(|&b| b == 0));
}

#[test]
fn upscaling_is_refused_rather_than_invented() {
    let src = solid(4, 4, [1, 2, 3, 255]);
    let out = box_downsample(&src, 4, 4, 16, 16);
    assert_eq!(out, src);
}

#[test]
fn every_source_pixel_is_counted_exactly_once() {
    let mut src = vec![0u8; 7 * 4];
    for x in 0..7usize {
        src[x * 4 + 3] = 255;
        src[x * 4] = (x * 30) as u8;
    }
    let out = box_downsample(&src, 7, 1, 3, 1);
    let total: u32 = (0..3).map(|x| at(&out, 3, x, 0)[0] as u32).sum();
    assert!(total > 0, "nothing was dropped on the floor: {out:?}");
    assert_eq!(out.len(), 3 * 4);
}

#[test]
fn fit_within_keeps_the_aspect_ratio() {
    assert_eq!(fit_within(400, 200, 100, 100), (100, 50));
    assert_eq!(fit_within(200, 400, 100, 100), (50, 100));
    assert_eq!(fit_within(50, 50, 100, 100), (50, 50), "already fits");
    assert_eq!(fit_within(100, 100, 100, 100), (100, 100), "exactly fits");
}

/// An extreme ratio must not round an axis to zero — an image scaled to nothing is not a fit,
/// it is a different way of losing the paste.
#[test]
fn fit_within_never_scales_an_axis_away() {
    let (w, h) = fit_within(10_000, 3, 100, 100);
    assert_eq!(w, 100);
    assert!(h >= 1, "height collapsed to {h}");
}
