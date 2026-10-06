use super::{accent_or_seed, now_secs, ProjectStore, StoreError};
use crate::{adjustments_blob, guides_blob, text_blob, transform_blob, vector_blob};
use calumma_core::tile::{self, DirtyChannel, TileCoord, TILE_BYTES};
use calumma_core::{BlendMode, Document, Layer};
use rusqlite::{params, OptionalExtension};
use rustc_hash::FxHashMap;
use std::sync::Arc;
use uuid::Uuid;

/// What `layers.content_kind` means on disk. Text is 2; a row claiming to be text whose blob
/// will not decode falls back to raster so a damaged or newer file loses the run, not the
/// whole project.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LayerKind {
    Raster,
    Vector,
    Text,
}

impl ProjectStore {
    pub fn open_project(&self, id: &str) -> Result<Document, StoreError> {
        let ts = now_secs();
        self.conn.execute(
            "UPDATE projects SET opened_at = ?1 WHERE id = ?2",
            params![ts, id],
        )?;
        let (name, width, height, accent, guides): (String, u32, u32, [u8; 3], Option<Vec<u8>>) =
            self.conn
                .query_row(
                    "SELECT name, width, height, accent, guides FROM projects WHERE id = ?1",
                    params![id],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get::<_, i64>(1)? as u32,
                            row.get::<_, i64>(2)? as u32,
                            accent_or_seed(row.get::<_, Option<i64>>(3)?, id),
                            row.get::<_, Option<Vec<u8>>>(4)?,
                        ))
                    },
                )
                .optional()?
                .ok_or(StoreError::NotFound)?;

        let mut doc = Document::new(id.to_string(), name, width, height);
        doc.accent = accent;
        doc.set_guides(
            guides
                .as_deref()
                .and_then(guides_blob::decode)
                .unwrap_or_default(),
        );
        doc.layers.clear();

        let mut layer_stmt = self.conn.prepare(
            "SELECT layer_id, name, visible, content_kind, vector_data, opacity, blend_mode, adjustments, text_data, transform, locked, clips_to, clip_invert FROM layers WHERE project_id = ?1 ORDER BY z_index ASC",
        )?;
        let layer_rows = layer_stmt.query_map(params![id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)? != 0,
                row.get::<_, i64>(3)?,
                row.get::<_, Option<Vec<u8>>>(4)?,
                row.get::<_, f64>(5)? as f32,
                row.get::<_, i64>(6)? as u32,
                row.get::<_, Option<Vec<u8>>>(7)?,
                row.get::<_, Option<Vec<u8>>>(8)?,
                row.get::<_, Option<Vec<u8>>>(9)?,
                row.get::<_, i64>(10)? != 0,
                row.get::<_, Option<String>>(11)?,
                row.get::<_, i64>(12)? != 0,
            ))
        })?;

        let mut solid_tiles: FxHashMap<[u8; 4], Arc<Vec<u8>>> = FxHashMap::default();
        let mut tile_stmt = self
            .conn
            .prepare("SELECT tx, ty, pixels FROM tiles WHERE project_id = ?1 AND layer_id = ?2")?;

        for layer_row in layer_rows {
            let (
                layer_id,
                name,
                visible,
                content_kind,
                vector_data,
                opacity,
                blend_mode,
                adjustments,
                text_data,
                transform,
                locked,
                clips_to,
                clip_invert,
            ) = layer_row?;
            let decoded_run = text_data.as_deref().and_then(text_blob::decode);
            let kind = match (content_kind, &decoded_run) {
                (1, _) => LayerKind::Vector,
                (2, Some(_)) => LayerKind::Text,
                _ => LayerKind::Raster,
            };
            let mut layer = match kind {
                LayerKind::Vector => {
                    let items = vector_data
                        .as_deref()
                        .and_then(vector_blob::decode)
                        .unwrap_or_default();
                    if items.is_empty() {
                        continue;
                    }
                    let mut first = true;
                    for item in items {
                        let mut layer = Layer::vector(name.clone(), item);
                        layer.id = if first {
                            first = false;
                            layer_id.clone()
                        } else {
                            Uuid::new_v4().to_string()
                        };
                        layer.visible = visible;
                        layer.opacity = opacity.clamp(0.0, 1.0);
                        layer.blend_mode = BlendMode::from_u32(blend_mode).unwrap_or_default();
                        layer.adjustments =
                            adjustments.as_deref().and_then(adjustments_blob::decode);
                        layer.transform = transform.as_deref().and_then(transform_blob::decode);
                        layer.locked = locked;
                        if first {
                            layer.clips_to = clips_to.clone();
                            layer.clip_invert = clip_invert;
                        }
                        doc.layers.push(layer);
                    }
                    continue;
                }
                LayerKind::Text => {
                    let run = decoded_run.unwrap_or_default();
                    let mut layer = Layer::text(name, run, width, height);
                    layer.id = layer_id.clone();
                    layer
                }
                LayerKind::Raster => Layer::with_id(layer_id.clone(), name, width, height),
            };
            layer.visible = visible;
            layer.opacity = opacity.clamp(0.0, 1.0);
            layer.blend_mode = BlendMode::from_u32(blend_mode).unwrap_or_default();
            layer.adjustments = adjustments.as_deref().and_then(adjustments_blob::decode);
            layer.transform = transform.as_deref().and_then(transform_blob::decode);
            layer.locked = locked;
            layer.clips_to = clips_to;
            layer.clip_invert = clip_invert;
            if kind == LayerKind::Text {
                layer.clear_dirty(DirtyChannel::Store);
            }
            if kind == LayerKind::Raster {
                let tile_rows = tile_stmt.query_map(params![id, layer_id], |row| {
                    Ok((
                        row.get::<_, i64>(0)? as i32,
                        row.get::<_, i64>(1)? as i32,
                        row.get::<_, Vec<u8>>(2)?,
                    ))
                })?;
                for tile in tile_rows {
                    let (tx, ty, pixels) = tile?;
                    if pixels.len() != TILE_BYTES {
                        continue;
                    }
                    let coord = TileCoord { x: tx, y: ty };
                    let Some(grid) = layer.tiles_mut() else {
                        continue;
                    };
                    // A layer that overflows the canvas — a pasted image bigger than the paper —
                    // stores tiles outside the document rectangle, and a fresh grid holds only
                    // the document. Widen it to admit the tile before asking to insert it,
                    // otherwise reopening a project is where the overflow quietly disappears.
                    grid.grow_extent_to_tile(coord);
                    // Paper comes back as hundreds of identical white blobs; giving them one
                    // shared allocation costs a scan that mixed tiles abandon immediately.
                    match tile::uniform_color(&pixels) {
                        Some(color) => {
                            let shared = solid_tiles
                                .entry(color)
                                .or_insert_with(|| Arc::new(pixels))
                                .clone();
                            grid.insert_shared(coord, shared);
                        }
                        None => {
                            if let Some(dst) = grid.ensure_mut(coord) {
                                dst.copy_from_slice(&pixels);
                            }
                        }
                    }
                }
                layer.clear_dirty(DirtyChannel::Store);
            }
            doc.layers.push(layer);
        }

        if doc.layers.is_empty() {
            doc.layers
                .push(Layer::new(calumma_core::LAYER_ONE, width, height));
        }
        let missing_paper = !doc.layers.iter().any(Layer::is_paper);
        doc.ensure_paper_layer();
        doc.active_layer = doc
            .layers
            .iter()
            .position(|l| l.content.is_raster() && !l.is_paper())
            .or_else(|| doc.layers.iter().position(|l| l.content.is_raster()))
            .unwrap_or(0);
        if missing_paper {
            self.save(&mut doc)?;
        }
        Ok(doc)
    }
}
