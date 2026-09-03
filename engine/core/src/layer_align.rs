use crate::document::Document;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlignEdge {
    Left,
    CenterH,
    Right,
    Top,
    CenterV,
    Bottom,
}

impl Document {
    pub fn align_layers(&mut self, indices: &[usize], edge: AlignEdge) -> bool {
        let indices = Self::alignable_indices(self, indices);
        if indices.len() < 2 {
            return false;
        }
        let Some(target) = union_layer_aabbs(self, &indices) else {
            return false;
        };
        let mut aligned = false;
        for index in indices {
            let Some(layer) = self.layers.get(index) else {
                continue;
            };
            let Some(raw) = layer.content_bounds() else {
                continue;
            };
            let t = layer.transform.unwrap_or_default();
            let aabb = t.transformed_aabb(raw);
            let (dx, dy) = align_delta(edge, target, aabb);
            if dx == 0.0 && dy == 0.0 {
                continue;
            }
            let mut next = t;
            next.offset_x += dx;
            next.offset_y += dy;
            if let Some(layer) = self.layers.get_mut(index) {
                layer.transform = Some(next.clamped());
                aligned = true;
            }
        }
        aligned
    }

    fn alignable_indices(doc: &Document, indices: &[usize]) -> Vec<usize> {
        let mut out: Vec<usize> = indices
            .iter()
            .copied()
            .filter(|&index| doc.layer_movable(index))
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }
}

fn union_layer_aabbs(doc: &Document, indices: &[usize]) -> Option<(f32, f32, f32, f32)> {
    let mut union: Option<(f32, f32, f32, f32)> = None;
    for &index in indices {
        let layer = doc.layers.get(index)?;
        let raw = layer.content_bounds()?;
        let aabb = layer
            .transform
            .unwrap_or_default()
            .transformed_aabb(raw);
        union = Some(match union {
            None => aabb,
            Some((x0, y0, x1, y1)) => (
                x0.min(aabb.0),
                y0.min(aabb.1),
                x1.max(aabb.2),
                y1.max(aabb.3),
            ),
        });
    }
    union
}

fn align_delta(
    edge: AlignEdge,
    target: (f32, f32, f32, f32),
    aabb: (f32, f32, f32, f32),
) -> (f32, f32) {
    match edge {
        AlignEdge::Left => (target.0 - aabb.0, 0.0),
        AlignEdge::Right => (target.2 - aabb.2, 0.0),
        AlignEdge::CenterH => {
            let target_center = (target.0 + target.2) * 0.5;
            let layer_center = (aabb.0 + aabb.2) * 0.5;
            (target_center - layer_center, 0.0)
        }
        AlignEdge::Top => (0.0, target.1 - aabb.1),
        AlignEdge::Bottom => (0.0, target.3 - aabb.3),
        AlignEdge::CenterV => {
            let target_center = (target.1 + target.3) * 0.5;
            let layer_center = (aabb.1 + aabb.3) * 0.5;
            (0.0, target_center - layer_center)
        }
    }
}
