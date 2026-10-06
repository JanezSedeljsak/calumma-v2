use crate::tile_atlas::tile_mip_levels;
use calumma_core::tile::{TileCoord, TILE_BYTES, TILE_SIZE};
use calumma_core::Layer;

/// Clip alpha baked into a tile before upload — the one remaining CPU fold. Adjustments and
/// opacity are evaluated on the GPU (`LayerData.tone`/`opacity` in `board.wgsl`'s `fs_tile`).
/// Returns `None` when the layer is not clipped and the tile's own bytes can go to the GPU
/// untouched — the common case, and the reason this allocates only for clipped layers.
pub fn composited_tile_payload(
    pixels: &[u8],
    coord: TileCoord,
    layer: &Layer,
    clip_base: Option<&Layer>,
) -> Option<Vec<u8>> {
    let base = clip_base?;
    let mut out = Vec::with_capacity(TILE_BYTES);
    out.extend_from_slice(pixels);
    out.resize(TILE_BYTES, 0);
    let (ox, oy) = coord.origin();
    for ty in 0..TILE_SIZE {
        for tx in 0..TILE_SIZE {
            let x = ox + tx as i32;
            let y = oy + ty as i32;
            let i = ((ty * TILE_SIZE + tx) * 4) as usize;
            let base_alpha = calumma_core::clip::clip_base_alpha_for_layer_pixel(layer, base, x, y);
            out[i + 3] =
                calumma_core::clip::apply_clip_alpha(out[i + 3], base_alpha, layer.clip_invert);
        }
    }
    Some(out)
}

/// A tile's full mip chain — `base` as level 0, then each level halved down to 1×1 — for
/// `TileAtlas::write`. Panning or zooming a document out samples these coarser levels instead
/// of raw 256×256 texels through a plain bilinear filter, which is what shimmering/moiré during
/// a pan actually is: minification aliasing from sampling a texture well below its own
/// resolution with nothing pre-filtered to fall back to.
pub fn tile_mip_chain(base: &[u8]) -> Vec<Vec<u8>> {
    let mut out = vec![base.to_vec()];
    out.extend(tile_upload_mips(base, false));
    out
}

/// The mip chain *above* level 0, which the caller already holds. Empty during camera motion,
/// where the base level is all that gets written.
pub fn tile_upload_mips(base: &[u8], motion: bool) -> Vec<Vec<u8>> {
    if motion {
        return Vec::new();
    }
    let levels = tile_mip_levels();
    let mut out: Vec<Vec<u8>> = Vec::with_capacity(levels as usize - 1);
    let mut side = TILE_SIZE;
    for _ in 1..levels {
        let prev: &[u8] = out.last().map_or(base, Vec::as_slice);
        out.push(downsample_box(prev, side));
        side = (side / 2).max(1);
    }
    debug_assert_eq!(out.len() + 1, levels as usize);
    out
}

/// One 2×2 box-filter pass, straight (non-premultiplied) alpha in and out. Averaging straight
/// RGB directly would let a fully transparent neighbour's color bleed into a translucent or
/// opaque one — invisible pixels are not "no color", they are color nobody sees yet — so each
/// tap is weighted by its own alpha (premultiplied) before averaging, and the result is
/// unpremultiplied back out. This is the same reason blending happens in premultiplied space
/// everywhere else in this renderer.
fn downsample_box(src: &[u8], side: u32) -> Vec<u8> {
    let out_side = (side / 2).max(1);
    let mut out = vec![0u8; (out_side * out_side * 4) as usize];
    for dy in 0..out_side {
        for dx in 0..out_side {
            let sx0 = (dx * 2).min(side - 1);
            let sy0 = (dy * 2).min(side - 1);
            let sx1 = (dx * 2 + 1).min(side - 1);
            let sy1 = (dy * 2 + 1).min(side - 1);

            let mut sum_rgb = [0.0f32; 3];
            let mut sum_a = 0.0f32;
            for (sx, sy) in [(sx0, sy0), (sx1, sy0), (sx0, sy1), (sx1, sy1)] {
                let i = ((sy * side + sx) * 4) as usize;
                let a = src[i + 3] as f32 / 255.0;
                sum_rgb[0] += src[i] as f32 * a;
                sum_rgb[1] += src[i + 1] as f32 * a;
                sum_rgb[2] += src[i + 2] as f32 * a;
                sum_a += a;
            }

            let out_a = sum_a / 4.0;
            let rgb = if out_a > 0.0 {
                sum_rgb.map(|c| (c / (out_a * 4.0)).round().clamp(0.0, 255.0) as u8)
            } else {
                [0u8; 3]
            };
            let di = ((dy * out_side + dx) * 4) as usize;
            out[di..di + 3].copy_from_slice(&rgb);
            out[di + 3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}
