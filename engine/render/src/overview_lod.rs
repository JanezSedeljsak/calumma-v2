use calumma_core::limits::{OVERVIEW_CHUNK_TILES, OVERVIEW_COARSEST_SIDE, OVERVIEW_LEVELS};
use calumma_core::tile::{DirtyChannel, TileCoord, TILE_SIZE};
use calumma_core::{Document, VectorItem};
use rustc_hash::{FxHashSet, FxHasher};
use std::hash::{Hash, Hasher};

pub(crate) fn pyramid_sides(width: u32, height: u32, finest_cap: u32) -> Vec<u32> {
    let long = width.max(height).max(1);
    let mut side = long.min(finest_cap).max(1);
    let mut sides = vec![side];
    while sides.len() < OVERVIEW_LEVELS && side > OVERVIEW_COARSEST_SIDE {
        let next = (side / 2).max(1);
        if next == side {
            break;
        }
        side = next;
        sides.push(side);
    }
    sides
}

pub(crate) fn needed_side(doc: &Document) -> u32 {
    let long = doc.width.max(doc.height).max(1) as f32;
    (long * doc.camera.zoom * doc.camera.dpr).ceil().max(1.0) as u32
}

pub(crate) fn pick_level(sides: &[u32], needed: u32) -> usize {
    let Some(&finest) = sides.first() else {
        return 0;
    };
    let needed = needed.min(finest);
    let mut chosen = 0;
    for (i, &side) in sides.iter().enumerate() {
        if side >= needed {
            chosen = i;
        } else {
            break;
        }
    }
    chosen
}

fn chunk_of(coord: TileCoord) -> (i32, i32) {
    (
        coord.x.div_euclid(OVERVIEW_CHUNK_TILES),
        coord.y.div_euclid(OVERVIEW_CHUNK_TILES),
    )
}

pub(crate) fn overview_chunks(doc: &Document) -> FxHashSet<(i32, i32)> {
    let mut chunks = FxHashSet::default();
    for layer in &doc.layers {
        let Some(tiles) = layer.dirty_tiles(DirtyChannel::Overview) else {
            continue;
        };
        for coord in tiles {
            chunks.insert(chunk_of(*coord));
        }
    }
    chunks
}

pub(crate) fn chunk_tex_rect(
    chunk_x: i32,
    chunk_y: i32,
    dw: u32,
    dh: u32,
    tw: u32,
    th: u32,
) -> Option<(u32, u32, u32, u32)> {
    let chunk = OVERVIEW_CHUNK_TILES * TILE_SIZE as i32;
    let doc_x0 = (chunk_x * chunk).max(0) as u32;
    let doc_y0 = (chunk_y * chunk).max(0) as u32;
    if doc_x0 >= dw || doc_y0 >= dh {
        return None;
    }
    let doc_x1 = (doc_x0 + chunk as u32).min(dw);
    let doc_y1 = (doc_y0 + chunk as u32).min(dh);
    let x0 = doc_to_tex(doc_x0, dw, tw).saturating_sub(1);
    let y0 = doc_to_tex(doc_y0, dh, th).saturating_sub(1);
    let x1 = (doc_to_tex(doc_x1.saturating_sub(1), dw, tw) + 2).min(tw);
    let y1 = (doc_to_tex(doc_y1.saturating_sub(1), dh, th) + 2).min(th);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    Some((x0, y0, x1 - x0, y1 - y0))
}

fn doc_to_tex(doc: u32, dim: u32, tex: u32) -> u32 {
    if tex <= 1 || dim <= 1 {
        return 0;
    }
    ((doc as u64) * (tex as u64 - 1) / (dim as u64 - 1)) as u32
}

pub(crate) fn stack_stamp(doc: &Document) -> u64 {
    let mut hasher = FxHasher::default();
    doc.layers.len().hash(&mut hasher);
    for layer in &doc.layers {
        layer.id.hash(&mut hasher);
        layer.visible.hash(&mut hasher);
        layer.opacity.to_bits().hash(&mut hasher);
        layer.blend_mode.as_u32().hash(&mut hasher);
        hash_mask(layer.mask(), &mut hasher);
        match layer.transform {
            Some(t) => {
                t.offset_x.to_bits().hash(&mut hasher);
                t.offset_y.to_bits().hash(&mut hasher);
                t.scale_x.to_bits().hash(&mut hasher);
                t.scale_y.to_bits().hash(&mut hasher);
                t.rotation.to_bits().hash(&mut hasher);
            }
            None => 0u8.hash(&mut hasher),
        }
        match layer.adjustments {
            Some(a) => {
                a.brightness.to_bits().hash(&mut hasher);
                a.contrast.to_bits().hash(&mut hasher);
                a.vibrance.to_bits().hash(&mut hasher);
                a.saturation.to_bits().hash(&mut hasher);
                a.levels_gamma.to_bits().hash(&mut hasher);
            }
            None => 0u8.hash(&mut hasher),
        }
        if let Some(item) = layer.content.item() {
            hash_vector_item(item, &mut hasher);
        }
    }
    hasher.finish()
}

fn hash_mask(mask: Option<&[u8]>, hasher: &mut FxHasher) {
    let Some(mask) = mask else {
        0u8.hash(hasher);
        return;
    };
    1u8.hash(hasher);
    mask.len().hash(hasher);
    if let Some(b) = mask.first() {
        b.hash(hasher);
    }
    if let Some(b) = mask.last() {
        b.hash(hasher);
    }
    let step = (mask.len() / 16).max(1);
    for b in mask.iter().step_by(step) {
        b.hash(hasher);
    }
}

fn hash_vector_item(item: &VectorItem, hasher: &mut FxHasher) {
    item.color().hash(hasher);
    item.stroke_color().hash(hasher);
    item.ink_pad().to_bits().hash(hasher);
    match item {
        VectorItem::Shape(s) => {
            0u8.hash(hasher);
            u32::from(s.shape.tool).hash(hasher);
            s.shape.start.0.to_bits().hash(hasher);
            s.shape.start.1.to_bits().hash(hasher);
            s.shape.end.0.to_bits().hash(hasher);
            s.shape.end.1.to_bits().hash(hasher);
            s.shape.half_width.to_bits().hash(hasher);
            s.shape.fill.hash(hasher);
            s.shape.stroke.hash(hasher);
        }
        VectorItem::Path(p) => {
            1u8.hash(hasher);
            p.closed.hash(hasher);
            p.fill.hash(hasher);
            p.stroke.hash(hasher);
            p.stroke_width.to_bits().hash(hasher);
            p.points.len().hash(hasher);
            for &(x, y) in &p.points {
                x.to_bits().hash(hasher);
                y.to_bits().hash(hasher);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use calumma_core::limits::OVERVIEW_FINEST_SIDE;
    use calumma_core::tile::DocRect;
    use calumma_core::vector::VectorShape;
    use calumma_core::{Adjustments, BlendMode, LayerTransform, Shape, Tool};

    fn doc(width: u32, height: u32) -> Document {
        Document::new("p".into(), "t", width, height)
    }

    #[test]
    fn a_low_tier_budget_caps_the_finest_level() {
        let sides = pyramid_sides(8192, 8192, 2048);
        assert_eq!(sides[0], 2048);
        assert!(sides.len() > 1);
        assert_eq!(*sides.last().unwrap(), OVERVIEW_COARSEST_SIDE);
    }

    #[test]
    fn a_standard_8k_pyramid_is_four_half_steps_from_the_finest_cap() {
        assert_eq!(
            pyramid_sides(8192, 8192, OVERVIEW_FINEST_SIDE),
            vec![4096, 2048, 1024, 512]
        );
    }

    #[test]
    fn a_document_smaller_than_the_coarsest_floor_is_one_level() {
        assert_eq!(pyramid_sides(128, 64, OVERVIEW_FINEST_SIDE), vec![128]);
    }

    #[test]
    fn pick_level_chooses_the_coarsest_side_that_still_covers_the_need() {
        let sides = [4096, 2048, 1024, 512];
        assert_eq!(pick_level(&sides, 300), 3);
        assert_eq!(pick_level(&sides, 512), 3);
        assert_eq!(pick_level(&sides, 513), 2);
        assert_eq!(pick_level(&sides, 1500), 1);
        assert_eq!(pick_level(&sides, 2048), 1);
        assert_eq!(pick_level(&sides, 2049), 0);
        assert_eq!(pick_level(&sides, 8000), 0);
        assert_eq!(pick_level(&[], 100), 0);
    }

    #[test]
    fn needed_side_is_the_document_long_edge_in_device_pixels() {
        let mut d = doc(1000, 400);
        d.camera.zoom = 0.5;
        d.camera.dpr = 2.0;
        assert_eq!(needed_side(&d), 1000);
        d.camera.zoom = 0.25;
        assert_eq!(needed_side(&d), 500);
    }

    #[test]
    fn an_origin_chunk_covers_the_matching_texel_window() {
        let (x, y, w, h) = chunk_tex_rect(0, 0, 4096, 4096, 1024, 1024).unwrap();
        assert_eq!((x, y), (0, 0));
        assert!(w > 0 && h > 0);
        assert!(x + w <= 1024 && y + h <= 1024);
    }

    #[test]
    fn a_chunk_past_the_document_is_dropped() {
        assert!(chunk_tex_rect(8, 0, 1024, 1024, 256, 256).is_none());
    }

    #[test]
    fn neighbouring_tiles_share_a_chunk_and_a_tile_1024px_away_does_not() {
        let mut d = doc(2048, 256);
        d.clear_layer_dirty(DirtyChannel::Overview);
        d.layers[1]
            .tiles_mut()
            .unwrap()
            .paint_rect(DocRect::new(10, 10, 20, 20), |_, _, _| {
                Some([255, 0, 0, 255])
            });
        d.layers[1]
            .tiles_mut()
            .unwrap()
            .paint_rect(DocRect::new(800, 10, 820, 30), |_, _, _| {
                Some([0, 255, 0, 255])
            });
        let same = overview_chunks(&d);
        assert_eq!(same.len(), 1);

        d.layers[1]
            .tiles_mut()
            .unwrap()
            .paint_rect(DocRect::new(1200, 10, 1220, 30), |_, _, _| {
                Some([0, 0, 255, 255])
            });
        let both = overview_chunks(&d);
        assert_eq!(both.len(), 2);
        assert!(both.contains(&(0, 0)));
        assert!(both.contains(&(1, 0)));
    }

    #[test]
    fn a_paint_does_not_move_the_stack_stamp() {
        let mut d = doc(64, 64);
        let before = stack_stamp(&d);
        d.layers[1]
            .tiles_mut()
            .unwrap()
            .paint_rect(DocRect::new(4, 4, 8, 8), |_, _, _| Some([9, 9, 9, 255]));
        assert_eq!(stack_stamp(&d), before);
    }

    #[test]
    fn hiding_a_layer_or_changing_its_opacity_moves_the_stamp() {
        let mut d = doc(64, 64);
        let before = stack_stamp(&d);
        d.layers[1].visible = false;
        assert_ne!(stack_stamp(&d), before);
        d.layers[1].visible = true;
        assert_eq!(stack_stamp(&d), before);
        d.layers[1].opacity = 0.5;
        assert_ne!(stack_stamp(&d), before);
        d.layers[1].opacity = 1.0;
        d.layers[1].blend_mode = BlendMode::Multiply;
        assert_ne!(stack_stamp(&d), before);
    }

    #[test]
    fn recoloring_a_vector_moves_the_stamp_without_moving_its_box() {
        let mut d = doc(64, 64);
        d.add_vector_layer(
            "V",
            VectorItem::Shape(VectorShape {
                shape: Shape {
                    tool: Tool::Rect,
                    start: (8.0, 8.0),
                    end: (40.0, 40.0),
                    half_width: 1.0,
                    fill: true,
                    stroke: false,
                },
                color: [0, 90, 220, 255],
                stroke_color: [0, 90, 220, 255],
            }),
        );
        let before = stack_stamp(&d);
        let Some(VectorItem::Shape(shape)) = d.layers.last_mut().unwrap().content.item_mut() else {
            panic!("vector layer");
        };
        shape.color = [220, 30, 30, 255];
        assert_ne!(stack_stamp(&d), before);
    }

    #[test]
    fn a_thicker_stroke_or_a_fill_flag_moves_the_stamp() {
        let mut d = doc(64, 64);
        d.add_vector_layer(
            "V",
            VectorItem::Shape(VectorShape {
                shape: Shape {
                    tool: Tool::Rect,
                    start: (8.0, 8.0),
                    end: (40.0, 40.0),
                    half_width: 1.0,
                    fill: true,
                    stroke: false,
                },
                color: [0, 90, 220, 255],
                stroke_color: [0, 90, 220, 255],
            }),
        );
        let before = stack_stamp(&d);
        {
            let Some(VectorItem::Shape(shape)) = d.layers.last_mut().unwrap().content.item_mut()
            else {
                panic!("vector layer");
            };
            shape.shape.half_width = 4.0;
        }
        assert_ne!(stack_stamp(&d), before);
        let after_width = stack_stamp(&d);
        {
            let Some(VectorItem::Shape(shape)) = d.layers.last_mut().unwrap().content.item_mut()
            else {
                panic!("vector layer");
            };
            shape.shape.stroke = true;
        }
        assert_ne!(stack_stamp(&d), after_width);
    }

    #[test]
    fn a_transform_or_adjustment_moves_the_stamp() {
        let mut d = doc(64, 64);
        let before = stack_stamp(&d);
        d.layers[1].transform = Some(LayerTransform {
            offset_x: 12.0,
            ..LayerTransform::default()
        });
        assert_ne!(stack_stamp(&d), before);
        d.layers[1].transform = None;
        assert_eq!(stack_stamp(&d), before);
        d.layers[1].adjustments = Some(Adjustments {
            brightness: 0.2,
            ..Adjustments::default()
        });
        assert_ne!(stack_stamp(&d), before);
    }

    #[test]
    fn attaching_a_mask_moves_the_stamp() {
        let mut d = doc(8, 8);
        let before = stack_stamp(&d);
        d.layers[1].set_mask(Some(vec![255u8; 8 * 8]));
        assert_ne!(stack_stamp(&d), before);
        let mut mask = vec![255u8; 8 * 8];
        mask[0] = 0;
        d.layers[1].set_mask(Some(mask));
        let with_hole = stack_stamp(&d);
        d.layers[1].set_mask(Some(vec![255u8; 8 * 8]));
        assert_ne!(stack_stamp(&d), with_hole);
    }

    #[test]
    fn the_next_chunk_on_the_texture_starts_after_the_origin_chunk() {
        let origin = chunk_tex_rect(0, 0, 4096, 4096, 1024, 1024).unwrap();
        let next = chunk_tex_rect(1, 0, 4096, 4096, 1024, 1024).unwrap();
        assert!(next.0 > 0);
        assert!(next.0 < origin.0 + origin.2);
        assert_eq!(next.1, origin.1);
        assert!(next.0 + next.2 <= 1024);
    }

    #[test]
    fn needed_side_grows_with_dpr() {
        let mut d = doc(2048, 1024);
        d.camera.zoom = 0.25;
        d.camera.dpr = 1.0;
        let one = needed_side(&d);
        d.camera.dpr = 2.0;
        assert_eq!(needed_side(&d), one * 2);
    }
}
