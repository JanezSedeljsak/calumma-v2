use super::{pack_accent, ProjectStore, StoreError};
use crate::{adjustments_blob, guides_blob, text_blob, transform_blob, vector_blob};
use calumma_core::limits::PROJECT_THUMB_MAX_SIDE;
use calumma_core::tile::DirtyChannel;
use calumma_core::{Document, Layer, LayerContent};
use rusqlite::{params, params_from_iter};

/// The content-shaped columns of one `layers` row, which all follow from its content.
struct LayerColumns {
    content_kind: i64,
    vector_data: Option<Vec<u8>>,
    text_data: Option<Vec<u8>>,
}

impl LayerColumns {
    fn of(layer: &Layer) -> Self {
        match &layer.content {
            LayerContent::Raster(_) => Self {
                content_kind: 0,
                vector_data: None,
                text_data: None,
            },
            LayerContent::Vector(item) => Self {
                content_kind: 1,
                vector_data: Some(vector_blob::encode(item)),
                text_data: None,
            },
            LayerContent::Text { run, .. } => Self {
                content_kind: 2,
                vector_data: None,
                text_data: Some(text_blob::encode(run)),
            },
        }
    }
}

fn placeholders(count: usize) -> String {
    (0..count)
        .map(|i| format!("?{}", i + 2))
        .collect::<Vec<_>>()
        .join(",")
}

impl ProjectStore {
    pub fn save(&self, doc: &mut Document) -> Result<(), StoreError> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE projects SET name = ?1, width = ?2, height = ?3, accent = ?4, guides = ?5 WHERE id = ?6",
            params![
                doc.name,
                doc.width as i64,
                doc.height as i64,
                pack_accent(doc.accent),
                guides_blob::encode(doc.guides()),
                doc.id
            ],
        )?;

        let live_ids: Vec<String> = doc.layers.iter().map(|l| l.id.clone()).collect();
        let prune = format!(
            "DELETE FROM layers WHERE project_id = ?1 AND layer_id NOT IN ({})",
            placeholders(live_ids.len())
        );
        let mut prune_args: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(live_ids.len() + 1);
        prune_args.push(&doc.id);
        for id in &live_ids {
            prune_args.push(id);
        }
        tx.execute(&prune, params_from_iter(prune_args))?;

        {
            let mut upsert_layer = tx.prepare(
                "INSERT INTO layers (project_id, layer_id, name, visible, z_index, content_kind, vector_data, opacity, blend_mode, adjustments, text_data, transform, locked, clips_to, clip_invert) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
                 ON CONFLICT(project_id, layer_id) DO UPDATE SET
                    name = excluded.name,
                    visible = excluded.visible,
                    z_index = excluded.z_index,
                    content_kind = excluded.content_kind,
                    vector_data = excluded.vector_data,
                    opacity = excluded.opacity,
                    blend_mode = excluded.blend_mode,
                    adjustments = excluded.adjustments,
                    text_data = excluded.text_data,
                    transform = excluded.transform,
                    locked = excluded.locked,
                    clips_to = excluded.clips_to,
                    clip_invert = excluded.clip_invert",
            )?;
            let mut upsert_tile = tx.prepare(
                "INSERT INTO tiles (project_id, layer_id, tx, ty, pixels) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(project_id, layer_id, tx, ty) DO UPDATE SET pixels = excluded.pixels",
            )?;
            let mut delete_tile = tx.prepare(
                "DELETE FROM tiles WHERE project_id = ?1 AND layer_id = ?2 AND tx = ?3 AND ty = ?4",
            )?;

            for (z, layer) in doc.layers.iter().enumerate() {
                let LayerColumns {
                    content_kind,
                    vector_data,
                    text_data,
                } = LayerColumns::of(layer);
                let adjustments = layer.adjustments.as_ref().map(adjustments_blob::encode);
                let transform = layer.transform.as_ref().map(transform_blob::encode);
                upsert_layer.execute(params![
                    doc.id,
                    layer.id,
                    layer.name,
                    if layer.visible { 1 } else { 0 },
                    z as i64,
                    content_kind,
                    vector_data,
                    layer.opacity as f64,
                    layer.blend_mode.as_u32() as i64,
                    adjustments,
                    text_data,
                    transform,
                    layer.locked as i64,
                    layer.clips_to,
                    layer.clip_invert as i64
                ])?;

                // A text layer's tiles are a cache of its run, so the run is all that is
                // written — re-rasterizing on open costs a millisecond and saves storing a
                // bitmap of every headline in the project.
                if layer.is_text() {
                    continue;
                }
                let Some(grid) = layer.tiles() else {
                    continue;
                };
                for coord in grid.dirty_tiles(DirtyChannel::Store) {
                    match grid.pixels_ref(*coord) {
                        Some(pixels) => {
                            upsert_tile
                                .execute(params![doc.id, layer.id, coord.x, coord.y, pixels])?;
                        }
                        None => {
                            delete_tile.execute(params![doc.id, layer.id, coord.x, coord.y])?;
                        }
                    }
                }
            }
        }

        tx.commit()?;
        doc.clear_layer_dirty(DirtyChannel::Store);
        self.write_project_thumbnail(doc)?;
        Ok(())
    }

    fn write_project_thumbnail(&self, doc: &Document) -> Result<(), StoreError> {
        let (w, h, rgba) = doc.composite_thumbnail(PROJECT_THUMB_MAX_SIDE);
        let png = crate::encode_png_rgba(&rgba, w, h).map_err(|e| StoreError::Io(e.into()))?;
        self.conn.execute(
            "UPDATE projects SET thumb = ?1 WHERE id = ?2",
            params![png, doc.id],
        )?;
        Ok(())
    }
}
