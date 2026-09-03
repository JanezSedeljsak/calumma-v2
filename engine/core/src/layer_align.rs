use crate::document::Document;

/// The minimum number of layers a distribute pass needs: two of them are the fixed
/// extremes, so there has to be at least one box in between for a gap to equalize.
const MIN_DISTRIBUTE: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlignEdge {
    Left,
    CenterH,
    Right,
    Top,
    CenterV,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DistributeAxis {
    Horizontal,
    Vertical,
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
        self.record_transforms_for_indices(&indices);
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

    /// Equalizes the gaps between the selected layers along one axis. The first and last box
    /// on that axis stay where they are — the span they define is what the middle boxes get
    /// spread across — so distributing twice in a row is a no-op, which is the whole point.
    pub fn distribute_layers(&mut self, indices: &[usize], axis: DistributeAxis) -> bool {
        let indices = Self::alignable_indices(self, indices);
        if indices.len() < MIN_DISTRIBUTE {
            return false;
        }
        let mut boxes: Vec<(usize, (f32, f32, f32, f32))> = Vec::with_capacity(indices.len());
        for index in indices {
            let Some(layer) = self.layers.get(index) else {
                continue;
            };
            let Some(raw) = layer.content_bounds() else {
                continue;
            };
            let t = layer.transform.unwrap_or_default();
            boxes.push((index, t.transformed_aabb(raw)));
        }
        if boxes.len() < MIN_DISTRIBUTE {
            return false;
        }
        let indices: Vec<usize> = boxes.iter().map(|(index, _)| *index).collect();
        self.record_transforms_for_indices(&indices);
        boxes.sort_by(|a, b| {
            leading(axis, a.1)
                .partial_cmp(&leading(axis, b.1))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.0.cmp(&b.0))
        });
        let span_start = boxes.iter().fold(f32::INFINITY, |acc, (_, aabb)| {
            acc.min(leading(axis, *aabb))
        });
        let span_end = boxes.iter().fold(f32::NEG_INFINITY, |acc, (_, aabb)| {
            acc.max(trailing(axis, *aabb))
        });
        let occupied: f32 = boxes.iter().map(|(_, aabb)| extent(axis, *aabb)).sum();
        // Overlapping boxes make this negative, and that is correct: the overlap is then shared
        // out evenly instead of the pass refusing to run.
        let gap = (span_end - span_start - occupied) / (boxes.len() - 1) as f32;
        let mut cursor = span_start;
        let mut moved = false;
        for (index, aabb) in boxes {
            let delta = cursor - leading(axis, aabb);
            cursor += extent(axis, aabb) + gap;
            if delta == 0.0 {
                continue;
            }
            let Some(layer) = self.layers.get_mut(index) else {
                continue;
            };
            let mut next = layer.transform.unwrap_or_default();
            match axis {
                DistributeAxis::Horizontal => next.offset_x += delta,
                DistributeAxis::Vertical => next.offset_y += delta,
            }
            layer.transform = Some(next.clamped());
            moved = true;
        }
        moved
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
        let aabb = layer.transform.unwrap_or_default().transformed_aabb(raw);
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

fn leading(axis: DistributeAxis, aabb: (f32, f32, f32, f32)) -> f32 {
    match axis {
        DistributeAxis::Horizontal => aabb.0,
        DistributeAxis::Vertical => aabb.1,
    }
}

fn trailing(axis: DistributeAxis, aabb: (f32, f32, f32, f32)) -> f32 {
    match axis {
        DistributeAxis::Horizontal => aabb.2,
        DistributeAxis::Vertical => aabb.3,
    }
}

fn extent(axis: DistributeAxis, aabb: (f32, f32, f32, f32)) -> f32 {
    trailing(axis, aabb) - leading(axis, aabb)
}
