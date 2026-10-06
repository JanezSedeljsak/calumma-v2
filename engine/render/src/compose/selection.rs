use super::{brush_params, StrokeInstance};
use calumma_core::{BrushProfile, Document, Selection, SelectionShape, StrokePoint, Tool};

pub fn selection_rect_or_ellipse(doc: &Document) -> Option<([f32; 2], [f32; 2], Tool)> {
    match &doc.selection.as_ref()?.shape {
        SelectionShape::Rect { start, end } => {
            Some(([start.0, start.1], [end.0, end.1], Tool::Rect))
        }
        SelectionShape::Ellipse { start, end } => {
            Some(([start.0, start.1], [end.0, end.1], Tool::Ellipse))
        }
        SelectionShape::Lasso { .. } | SelectionShape::Mask(_) => None,
    }
}

pub fn selection_lasso_points(doc: &Document) -> Option<Vec<StrokePoint>> {
    let Selection {
        shape: SelectionShape::Lasso { points },
    } = doc.selection.as_ref()?
    else {
        return None;
    };
    let mut closed: Vec<StrokePoint> = points.iter().map(|&(x, y)| StrokePoint { x, y }).collect();
    if let Some(&first) = closed.first() {
        closed.push(first);
    }
    Some(closed)
}

/// Marching ants for a mask selection: the boundary the mask traced when it was committed,
/// one stroke instance per run.
///
/// The trace itself lives in the engine (`SelectionMask::trace_outline`) and is already
/// merged into maximal runs, so this is a straight mapping — the render pass never walks the
/// bitmap, no matter how large the selection is.
pub fn selection_mask_edges(
    doc: &Document,
    width: f32,
    color: [f32; 4],
) -> Option<Vec<StrokeInstance>> {
    let Selection {
        shape: SelectionShape::Mask(mask),
    } = doc.selection.as_ref()?
    else {
        return None;
    };
    Some(
        mask.outline()
            .iter()
            .map(|&segment| StrokeInstance {
                segment,
                color,
                brush: brush_params(width, &BrushProfile::HARD),
            })
            .collect(),
    )
}
