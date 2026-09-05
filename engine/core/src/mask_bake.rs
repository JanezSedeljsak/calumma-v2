use crate::document::{outside_bands, Document};
use crate::layer::Layer;
use crate::tile::TileGrid;
use crate::transform::{bounds_center, LayerTransform};

pub(crate) fn visible_doc_bounds_for_mask(
    tiles: &TileGrid,
    mask: &[u8],
    doc_w: u32,
    doc_h: u32,
    transform: Option<LayerTransform>,
) -> Option<(f32, f32, f32, f32)> {
    let crop = tiles.opaque_bounds()?;
    let pivot = bounds_center((
        crop.min_x as f32,
        crop.min_y as f32,
        crop.max_x as f32 + 1.0,
        crop.max_y as f32 + 1.0,
    ));
    let t = transform.unwrap_or_default();
    let has_transform = transform.is_some_and(|t| !t.is_identity());
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    let mut any = false;
    for y in crop.min_y..=crop.max_y {
        for x in crop.min_x..=crop.max_x {
            let px = tiles.get_pixel(x, y);
            if px[3] == 0 {
                continue;
            }
            let (doc_x, doc_y) = if has_transform {
                t.forward(pivot, (x as f32, y as f32))
            } else {
                (x as f32, y as f32)
            };
            let ix = doc_x.floor() as i32;
            let iy = doc_y.floor() as i32;
            if ix < 0 || iy < 0 || (ix as u32) >= doc_w || (iy as u32) >= doc_h {
                continue;
            }
            let index = (iy as u32 * doc_w + ix as u32) as usize;
            let m = mask.get(index).copied().unwrap_or(255);
            if ((px[3] as u32 * m as u32) / 255) == 0 {
                continue;
            }
            any = true;
            min_x = min_x.min(doc_x);
            min_y = min_y.min(doc_y);
            max_x = max_x.max(doc_x);
            max_y = max_y.max(doc_y);
        }
    }
    any.then_some((min_x, min_y, max_x + 1.0, max_y + 1.0))
}

fn preserve_doc_bounds(
    layer: &mut Layer,
    before: (f32, f32, f32, f32),
    transform: Option<LayerTransform>,
) {
    let Some(raw) = layer.content_bounds() else {
        return;
    };
    let Some(mut t) = transform else {
        return;
    };
    if t.rotation == 0.0 && t.scale_x == 1.0 && t.scale_y == 1.0 {
        t.offset_x = before.0 - raw.0;
        t.offset_y = before.1 - raw.1;
    } else {
        let after = t.transformed_aabb(raw);
        t.offset_x += before.0 - after.0;
        t.offset_y += before.1 - after.1;
    }
    layer.transform = (!t.is_identity()).then_some(t);
}

fn bake_mask_into_tiles(
    tiles: &mut TileGrid,
    mask: &[u8],
    doc_w: u32,
    doc_h: u32,
    transform: Option<LayerTransform>,
) {
    let Some(crop) = tiles.opaque_bounds() else {
        return;
    };
    let pivot = bounds_center((
        crop.min_x as f32,
        crop.min_y as f32,
        crop.max_x as f32 + 1.0,
        crop.max_y as f32 + 1.0,
    ));
    let t = transform.unwrap_or_default();
    let has_transform = transform.is_some_and(|t| !t.is_identity());
    for y in crop.min_y..=crop.max_y {
        for x in crop.min_x..=crop.max_x {
            let px = tiles.get_pixel(x, y);
            if px[3] == 0 {
                continue;
            }
            let (doc_x, doc_y) = if has_transform {
                t.forward(pivot, (x as f32, y as f32))
            } else {
                (x as f32, y as f32)
            };
            let ix = doc_x.floor() as i32;
            let iy = doc_y.floor() as i32;
            if ix < 0 || iy < 0 || (ix as u32) >= doc_w || (iy as u32) >= doc_h {
                tiles.set_pixel(x, y, [0, 0, 0, 0]);
                continue;
            }
            let index = (iy as u32 * doc_w + ix as u32) as usize;
            let m = mask.get(index).copied().unwrap_or(255);
            let alpha = ((px[3] as u32 * m as u32) / 255) as u8;
            if alpha == 0 {
                tiles.set_pixel(x, y, [0, 0, 0, 0]);
            } else {
                tiles.set_pixel(x, y, [px[0], px[1], px[2], alpha]);
            }
        }
    }
}

impl Document {
    pub fn apply_remove_background_mask(&mut self, layer_index: usize, mask: Vec<u8>) -> bool {
        let expected = (self.width as usize) * (self.height as usize);
        if mask.len() != expected || layer_index >= self.layers.len() {
            return false;
        }
        let layer = &self.layers[layer_index];
        if layer.tiles().is_none() || layer.is_paper() {
            return false;
        }
        let before_transform = layer.transform;
        let layer_id = layer.id.clone();
        let Some(tiles_ref) = layer.tiles() else {
            return false;
        };
        if tiles_ref.opaque_bounds().is_none() {
            return true;
        }
        let Some(before_doc) = visible_doc_bounds_for_mask(
            tiles_ref,
            &mask,
            self.width,
            self.height,
            before_transform,
        ) else {
            return false;
        };
        let Some(crop) = tiles_ref.opaque_bounds() else {
            return false;
        };
        let coords: Vec<_> = tiles_ref.coords_intersecting(crop).collect();
        let snap = tiles_ref.snapshot_tiles(&coords);
        let layer = &mut self.layers[layer_index];
        let Some(tiles) = layer.tiles_mut() else {
            return false;
        };
        bake_mask_into_tiles(tiles, &mask, self.width, self.height, before_transform);
        let Some(keep) = tiles.opaque_bounds() else {
            return false;
        };
        for band in outside_bands(tiles.bounds(), keep) {
            tiles.paint_rect(band, |_, _, _| Some([0, 0, 0, 0]));
        }
        layer.set_mask(None);
        preserve_doc_bounds(layer, before_doc, before_transform);
        self.history
            .push_remove_background(layer_id, snap, before_transform, Some(layer_index));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DocRect;
    use crate::Document;

    #[test]
    fn visible_doc_bounds_honours_layer_transform() {
        const DOC: u32 = 200;
        let mut doc = Document::new("p".into(), "t", DOC, DOC);
        doc.layers[1]
            .tiles_mut()
            .unwrap()
            .paint_rect(DocRect::new(10, 10, 90, 90), |_, _, _| {
                Some([255, 0, 0, 255])
            });
        doc.layers[1].transform = Some(LayerTransform {
            offset_x: 12.0,
            offset_y: -8.0,
            ..LayerTransform::default()
        });
        let mut mask = vec![255u8; (DOC as usize) * (DOC as usize)];
        for y in 0..DOC {
            for x in 0..DOC {
                if !(52..72).contains(&x) || !(32..52).contains(&y) {
                    mask[(y * DOC + x) as usize] = 0;
                }
            }
        }
        let layer = &doc.layers[1];
        let t = layer.transform.expect("transform set");
        assert_eq!(t.offset_x, 12.0);
        assert_eq!(t.offset_y, -8.0);
        assert!(!t.is_identity());
        let bounds =
            visible_doc_bounds_for_mask(layer.tiles().unwrap(), &mask, DOC, DOC, Some(t)).unwrap();
        assert_eq!(bounds, (52.0, 32.0, 72.0, 52.0));
    }

    /// Pins the one thing `preserve_doc_bounds`'s rotation/scale branch exists to get right:
    /// the *top-left corner* — where its offset correction anchors — lands exactly on
    /// `visible_doc_bounds_for_mask`'s tight, per-pixel pre-bake measurement.
    ///
    /// The bottom-right corner deliberately is not compared the same way: `layer_bounds` is
    /// `transform.transformed_aabb(content_bounds())`, an axis-aligned box drawn in *local*
    /// space and then rotated, while `visible_doc_bounds_for_mask` unions surviving pixels
    /// directly in *document* space. Under a rotation the two answer different questions — a
    /// rotated local rectangle's AABB is strictly looser than the tight union of the pixels
    /// inside it — so the far corner is only required to have grown, matching the near corner
    /// having stayed put, rather than to agree on a number the code was never trying to match.
    #[test]
    fn preserve_doc_bounds_anchors_the_top_left_corner_under_rotation_and_scale() {
        const DOC: u32 = 200;
        let mut doc = Document::new("p".into(), "t", DOC, DOC);
        doc.layers[1]
            .tiles_mut()
            .unwrap()
            .paint_rect(DocRect::new(10, 10, 90, 90), |_, _, _| {
                Some([255, 0, 0, 255])
            });
        let t = LayerTransform {
            offset_x: 20.0,
            offset_y: 5.0,
            scale_x: 1.5,
            scale_y: 1.5,
            rotation: 0.3,
        };
        doc.layers[1].transform = Some(t);
        assert!(!t.is_identity());

        let mut mask = vec![255u8; (DOC as usize) * (DOC as usize)];
        for y in 0..DOC {
            for x in 0..DOC {
                if !(40..60).contains(&x) || !(40..60).contains(&y) {
                    mask[(y * DOC + x) as usize] = 0;
                }
            }
        }
        let expected =
            visible_doc_bounds_for_mask(doc.layers[1].tiles().unwrap(), &mask, DOC, DOC, Some(t))
                .expect("the rotated/scaled subject is still on the canvas");

        assert!(doc.apply_remove_background_mask(1, mask));
        let after = doc.layer_bounds(1).expect("cropped rect, transformed");
        let close = |a: f32, b: f32| (a - b).abs() < 0.01;
        assert!(
            close(after.0, expected.0) && close(after.1, expected.1),
            "the near corner must land exactly where the pre-bake measurement put it: \
             after={after:?} expected={expected:?}"
        );
        assert!(
            after.2 >= expected.2 && after.3 >= expected.3,
            "the far corner of a local AABB rotated into document space can only be as tight \
             as or looser than the per-pixel union, never tighter: after={after:?} \
             expected={expected:?}"
        );
    }
}
