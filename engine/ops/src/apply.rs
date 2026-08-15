use crate::registry::OpRegistry;
use crate::types::{OpError, OpInput, OpKind, OpOutput, OpParams};
use calumma_core::{Document, Layer, LayerContent, VectorItem};

pub fn run_op(
    registry: &OpRegistry,
    kind: OpKind,
    input: OpInput,
    params: &OpParams,
) -> Result<OpOutput, OpError> {
    registry.run(kind, input, params)
}

pub fn run_op_on_document(
    registry: &OpRegistry,
    doc: &mut Document,
    layer_index: usize,
    kind: OpKind,
    params: &OpParams,
) -> Result<(), OpError> {
    let input = layer_input(doc, layer_index)?;
    let output = run_op(registry, kind, input, params)?;
    apply_output(doc, layer_index, output)
}

pub fn apply_output(
    doc: &mut Document,
    layer_index: usize,
    output: OpOutput,
) -> Result<(), OpError> {
    if layer_index >= doc.layers.len() {
        return Err(OpError::BadLayer);
    }
    match output {
        OpOutput::Mask(mask) => {
            let expected = (doc.width as usize) * (doc.height as usize);
            if mask.len() != expected {
                return Err(OpError::BadInput);
            }
            let layer = &mut doc.layers[layer_index];
            if !layer.content.is_raster() {
                return Err(OpError::BadLayer);
            }
            let before = layer.mask_owned();
            let layer_id = layer.id.clone();
            layer.set_mask(Some(mask));
            doc.history
                .push_layer_mask(layer_id, before, Some(layer_index));
            Ok(())
        }
        OpOutput::Raster { rgba, w, h } => {
            if w == 0 || h == 0 || rgba.len() != (w as usize) * (h as usize) * 4 {
                return Err(OpError::BadInput);
            }
            let mut layer = Layer::new(
                calumma_core::names::numbered_op_layer(doc.layers.len() + 1),
                w,
                h,
            );
            if let Some(tiles) = layer.tiles_mut() {
                tiles.blit_rgba(&rgba, w, h);
            }
            doc.layers.push(layer);
            doc.active_layer = doc.layers.len() - 1;
            Ok(())
        }
        OpOutput::Paths(paths) => {
            let layer = Layer::vector(
                calumma_core::names::numbered_vector_layer(doc.layers.len() + 1),
                paths.into_iter().map(VectorItem::Path).collect(),
            );
            doc.layers.push(layer);
            doc.active_layer = doc.layers.len() - 1;
            Ok(())
        }
    }
}

pub fn layer_input(doc: &Document, layer_index: usize) -> Result<OpInput, OpError> {
    let layer = doc.layers.get(layer_index).ok_or(OpError::BadLayer)?;
    match &layer.content {
        LayerContent::Raster(tiles) | LayerContent::Text { tiles, .. } => {
            let w = tiles.width;
            let h = tiles.height;
            let mut rgba = vec![0u8; (w as usize) * (h as usize) * 4];
            tiles.copy_into_rgba(&mut rgba, w, h);
            Ok(OpInput::Raster { rgba, w, h })
        }
        LayerContent::Vector(_) => Err(OpError::BadInput),
    }
}
