//! Replaying the visible layer stack into a content target — the draw list, the blend-mode
//! backdrop copies, and the pan cache's full redraw and strip patch.

use super::*;

/// Where a content replay lands: the texture itself, so a blend mode can copy what is under
/// it, and whether the replay starts from nothing or only clears its own scissor.
pub(in crate::renderer) struct ContentTarget<'t> {
    pub(in crate::renderer) texture: &'t wgpu::Texture,
    pub(in crate::renderer) view: &'t wgpu::TextureView,
    pub(in crate::renderer) clear_all: bool,
    pub(in crate::renderer) label: &'static str,
}

pub(super) fn content_pass<'e>(
    encoder: &'e mut wgpu::CommandEncoder,
    target: &ContentTarget<'_>,
    load: wgpu::LoadOp<wgpu::Color>,
) -> wgpu::RenderPass<'e> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(target.label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target.view,
            resolve_target: None,
            ops: wgpu::Operations {
                load,
                store: wgpu::StoreOp::Store,
            },
            depth_slice: None,
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        ..Default::default()
    })
}

pub(super) fn copy_region(
    encoder: &mut wgpu::CommandEncoder,
    from: &wgpu::Texture,
    to: &wgpu::Texture,
    rect: PxRect,
) {
    let width = rect.2.min(to.width().saturating_sub(rect.0));
    let height = rect.3.min(to.height().saturating_sub(rect.1));
    if width == 0 || height == 0 {
        return;
    }
    let at = |texture| wgpu::TexelCopyTextureInfo {
        texture,
        mip_level: 0,
        origin: wgpu::Origin3d {
            x: rect.0,
            y: rect.1,
            z: 0,
        },
        aspect: wgpu::TextureAspect::All,
    };
    encoder.copy_texture_to_texture(
        at(from),
        at(to),
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
}

impl Renderer {
    /// The whole layer stack as one ordered draw list, filling the instance buffers as it
    /// goes. A document layer's visible tiles become one range in `tiles` — one instanced
    /// draw call regardless of how many tiles that is. A vector layer is one item, so it is
    /// one `LayerDraw::Vector` and nothing in the layer coalesces.
    pub(in crate::renderer) fn build_layer_draws(
        &mut self,
        doc: &Document,
        tiles: &mut Vec<TileInstance>,
        vectors: &mut VectorInstances,
    ) -> Vec<LayerDraw> {
        let Some(visible) = doc.visible_rect() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (layer_index, layer) in doc.layers.iter().enumerate() {
            let layer_index = layer_index as u32;
            if !layer.visible {
                continue;
            }
            if doc.is_mask_base_id(&layer.id) {
                continue;
            }
            if let Some(item) = layer.content.item() {
                let placement = vector_placement(layer);
                if !item_visible(item, placement, visible) {
                    continue;
                }
                match item {
                    VectorItem::Shape(shape) => {
                        let start = vectors.shapes.len() as u32;
                        vectors.shapes.push(shape_instance(shape, placement));
                        out.push(LayerDraw::Vector(
                            VectorRun::Shapes,
                            start..vectors.shapes.len() as u32,
                        ));
                    }
                    VectorItem::Path(path) if path.fill && path.closed => {
                        let start = vectors.fills.len() as u32;
                        crate::vector_draw::push_fill_instance(
                            path,
                            placement,
                            &mut vectors.fills,
                            &mut vectors.fill_edges,
                        );
                        if vectors.fills.len() as u32 > start {
                            out.push(LayerDraw::Vector(
                                VectorRun::Fills,
                                start..vectors.fills.len() as u32,
                            ));
                        }
                    }
                    VectorItem::Path(path) => {
                        let start = vectors.strokes.len() as u32;
                        push_path_instances(path, placement, &mut vectors.strokes);
                        if vectors.strokes.len() as u32 > start {
                            out.push(LayerDraw::Vector(
                                VectorRun::Paths,
                                start..vectors.strokes.len() as u32,
                            ));
                        }
                    }
                }
                continue;
            }
            let Some(grid) = layer.tiles() else {
                continue;
            };
            let Some(slot) = self.layer_slots.get(&layer.id).copied() else {
                continue;
            };
            if layer.is_paper() && grid.whole_tiles_share_one_arc() {
                // `write_layer_data` already put this layer's atlas slot in its table row; the
                // draw only has to name the row.
                if self.tiles.contains_key(&(slot, 0, 0)) {
                    out.push(LayerDraw::Solid(layer.blend_mode, layer_index));
                }
                continue;
            }
            let visible_grid = layer.doc_rect_to_grid(visible);
            let start = tiles.len() as u32;
            for coord in grid.coords_intersecting(visible_grid) {
                let key: TileKey = (slot, coord.x, coord.y);
                let Some(gpu) = self.tiles.get(&key) else {
                    continue;
                };
                let (ox, oy) = coord.origin();
                tiles.push(TileInstance {
                    origin: [ox as f32, oy as f32],
                    slot: gpu.array_layer,
                    layer_index,
                });
            }
            if tiles.len() as u32 > start {
                out.push(LayerDraw::Tiles(
                    layer.blend_mode,
                    start..tiles.len() as u32,
                ));
            }
        }
        out
    }

    /// Replays `cached_draws` into `target` — the shared body behind both a full content redraw
    /// (the whole visible tile/vector set, into a fresh `PanCache` reference) and a blit-frame's
    /// exposed-strip repair (the same draws, scissored down to just the strip). Positions are
    /// document-space in the instance buffer, so the same buffers and draw calls reproduce
    /// correctly at any camera state — nothing here reads `doc` directly.
    ///
    /// It owns its render passes because a layer whose blend mode reads what is under it cannot
    /// be drawn in the pass that is still writing that: the pass ends, the scissored region is
    /// copied into the backdrop, and the layer is drawn against the copy in a pass of its own.
    /// A stack of Normal, Multiply and Screen layers stays one pass, as it always was.
    pub(in crate::renderer) fn draw_cached_content(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &ContentTarget<'_>,
        scissor: PxRect,
    ) {
        let mut draws = self.cached_draws.iter();
        let mut load = if target.clear_all {
            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
        } else {
            wgpu::LoadOp::Load
        };
        let mut clear_scissor = !target.clear_all;
        loop {
            let mut blended = None;
            {
                let mut pass = content_pass(encoder, target, load);
                pass.set_scissor_rect(scissor.0, scissor.1, scissor.2, scissor.3);
                if clear_scissor {
                    pass.set_pipeline(self.pan_cache.clear_pipeline());
                    pass.draw(0..3, 0..1);
                    clear_scissor = false;
                }
                for draw in draws.by_ref() {
                    if self.reads_backdrop(draw) {
                        blended = Some(draw);
                        break;
                    }
                    self.draw_layer(&mut pass, draw);
                }
            }
            let (Some(draw), Some(backdrop)) = (blended, self.backdrop.as_ref()) else {
                break;
            };
            copy_region(encoder, target.texture, &backdrop.texture, scissor);
            {
                let mut pass = content_pass(encoder, target, wgpu::LoadOp::Load);
                pass.set_scissor_rect(scissor.0, scissor.1, scissor.2, scissor.3);
                self.draw_blended_layer(&mut pass, draw, &backdrop.bind_group);
            }
            load = wgpu::LoadOp::Load;
        }
    }

    pub(super) fn reads_backdrop(&self, draw: &LayerDraw) -> bool {
        self.backdrop.is_some()
            && matches!(
                draw,
                LayerDraw::Tiles(mode, _) | LayerDraw::Solid(mode, _) if !mode.is_fixed_function()
            )
    }

    pub(super) fn draw_blended_layer(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        draw: &LayerDraw,
        backdrop: &wgpu::BindGroup,
    ) {
        match draw {
            LayerDraw::Tiles(_, range) => {
                pass.set_pipeline(&self.tile_blend_pipeline);
                pass.set_bind_group(0, self.atlas.bind_group(), &[]);
                pass.set_bind_group(1, backdrop, &[]);
                pass.set_vertex_buffer(0, self.tile_instance_buf.slice(..));
                pass.draw(0..6, range.clone());
            }
            LayerDraw::Solid(_, layer_index) => {
                pass.set_pipeline(&self.solid_blend_pipeline);
                pass.set_bind_group(0, self.atlas.bind_group(), &[]);
                pass.set_bind_group(1, backdrop, &[]);
                pass.draw(0..6, *layer_index..*layer_index + 1);
            }
            LayerDraw::Vector(..) => self.draw_layer(pass, draw),
        }
    }

    pub(super) fn draw_layer(&self, pass: &mut wgpu::RenderPass<'_>, draw: &LayerDraw) {
        {
            match draw {
                LayerDraw::Tiles(mode, range) => {
                    pass.set_pipeline(self.tile_pipeline(*mode));
                    pass.set_bind_group(0, self.atlas.bind_group(), &[]);
                    pass.set_vertex_buffer(0, self.tile_instance_buf.slice(..));
                    pass.draw(0..6, range.clone());
                }
                LayerDraw::Solid(mode, layer_index) => {
                    pass.set_pipeline(self.solid_pipeline(*mode));
                    pass.set_bind_group(0, self.atlas.bind_group(), &[]);
                    // The instance range *is* the argument: `vs_doc_quad` reads its layer row
                    // from `instance_index`, so a one-instance draw at `layer_index` says which.
                    pass.draw(0..6, *layer_index..*layer_index + 1);
                }
                LayerDraw::Vector(kind, range) => {
                    let (pipeline, bind_group, buf) = match kind {
                        VectorRun::Shapes => (
                            &self.vector_shape_pipeline,
                            &self.preview_bg,
                            &self.vector_shape_buf,
                        ),
                        VectorRun::Paths => {
                            (&self.stroke_pipeline, &self.preview_bg, &self.stroke_buf)
                        }
                        VectorRun::Fills => (
                            &self.vector_fill_pipeline,
                            &self.fill_bg,
                            &self.vector_fill_buf,
                        ),
                    };
                    pass.set_pipeline(pipeline);
                    pass.set_bind_group(0, bind_group, &[]);
                    pass.set_vertex_buffer(0, buf.slice(..));
                    pass.draw(0..6, range.clone());
                }
            }
        }
    }

    /// Draws the whole visible stack fresh into the `PanCache` reference texture, scissored to
    /// the current paper rect, and commits it as the new blit baseline. This is the "content
    /// pass" side of `ChunkDraw` in the plan's terms — a full redraw, just retargeted from the
    /// swapchain to an offscreen texture so a later camera-only frame has something to shift.
    pub(in crate::renderer) fn redraw_pan_cache_reference(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        pan: (f32, f32),
        zoom: f32,
        dpr: f32,
        scissor: PxRect,
    ) {
        let target = ContentTarget {
            texture: self.pan_cache.reference_texture(),
            view: self.pan_cache.reference_view(),
            clear_all: true,
            label: "pan-cache-full",
        };
        self.draw_cached_content(encoder, &target, scissor);
        self.pan_cache.commit_reference(pan, zoom, dpr, scissor);
    }

    /// Copies the previous frame's content shifted by this frame's pan delta into the
    /// `PanCache` working texture, patches the strips the copy could not have populated
    /// (`framebuffer::exposed_rects`) by replaying `cached_draws` scissored to just those
    /// rects, then promotes the result to be the next frame's reference. Each strip is cleared
    /// to transparent first — `LoadOp::Load` preserves the freshly copied region, so without an
    /// explicit clear a semi-transparent stroke in the strip would blend against whatever this
    /// texture held two frames ago instead of nothing.
    ///
    /// The promotion at the end is what keeps the strips thin: measured against the previous
    /// frame the exposed band is one frame's worth of travel, a few pixels on a normal drag.
    /// See `PanCache`'s own note.
    pub(in crate::renderer) fn patch_pan_cache_working(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        plan: framebuffer::BlitPlan,
        dpr: f32,
        scissor: PxRect,
    ) {
        let framebuffer::BlitPlan { src, dst, shift } = plan;
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: self.pan_cache.reference_texture(),
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: src.0,
                    y: src.1,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: self.pan_cache.working_texture(),
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: dst.0,
                    y: dst.1,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: src.2,
                height: src.3,
                depth_or_array_layers: 1,
            },
        );
        let target = ContentTarget {
            texture: self.pan_cache.working_texture(),
            view: self.pan_cache.working_view(),
            clear_all: false,
            label: "pan-cache-patch",
        };
        for strip in framebuffer::exposed_rects(scissor, dst)
            .into_iter()
            .flatten()
        {
            self.draw_cached_content(encoder, &target, strip);
        }
        self.pan_cache.commit_shift(shift, dpr, scissor);
    }
}
