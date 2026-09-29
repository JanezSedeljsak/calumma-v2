//! The one render pass that puts the frame on screen: paper, content, guides, previews,
//! chrome and the live brush, in that order.

use super::*;

/// What the board pass draws this frame, decided by the phases before it.
pub(super) struct BoardPass<'a> {
    pub(super) scissor: Option<PxRect>,
    pub(super) use_overview: bool,
    pub(super) has_content: bool,
    pub(super) guide_count: u32,
    pub(super) overlays: &'a OverlayRanges,
    pub(super) has_preview_shape: bool,
}

impl Renderer {
    pub(super) fn upload_draw_caches(&mut self, need_draw_rebuild: bool) {
        if need_draw_rebuild && !self.cached_shapes.is_empty() {
            self.ensure_vector_shape_capacity(self.cached_shapes.len());
            self.queue.write_buffer(
                &self.vector_shape_buf,
                0,
                bytemuck::cast_slice(&self.cached_shapes),
            );
        }
        if need_draw_rebuild && !self.cached_fills.is_empty() {
            self.ensure_vector_fill_capacity(self.cached_fills.len(), self.cached_fill_edges.len());
            self.queue.write_buffer(
                &self.vector_fill_buf,
                0,
                bytemuck::cast_slice(&self.cached_fills),
            );
            self.queue.write_buffer(
                &self.fill_edge_buf,
                0,
                bytemuck::cast_slice(&self.cached_fill_edges),
            );
        }
        if need_draw_rebuild && !self.cached_tile_instances.is_empty() {
            self.ensure_tile_instance_capacity(self.cached_tile_instances.len());
            self.queue.write_buffer(
                &self.tile_instance_buf,
                0,
                bytemuck::cast_slice(&self.cached_tile_instances),
            );
        }
    }

    pub(super) fn draw_board_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        doc: &Document,
        pass: BoardPass<'_>,
    ) {
        let BoardPass {
            scissor,
            use_overview,
            has_content,
            guide_count,
            overlays,
            has_preview_shape,
        } = pass;
        let overlay_range = &overlays.overlay;
        let screen_overlay_range = &overlays.screen;
        let brush_ring_range = &overlays.brush_ring;
        let brush_active = overlays.brush_active;
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("board"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                ..Default::default()
            });

            pass.set_pipeline(&self.paper_pipeline);
            pass.set_bind_group(0, &self.paper_bg, &[]);
            pass.draw(0..3, 0..1);

            if let Some((x, y, w, h)) = scissor {
                pass.set_scissor_rect(x, y, w, h);

                if use_overview {
                    self.overview.draw(&mut pass);
                } else if has_content {
                    pass.set_pipeline(self.pan_cache.blit_pipeline());
                    pass.set_bind_group(0, self.pan_cache.bind_group(), &[]);
                    pass.draw(0..3, 0..1);
                }
            }

            // Over the artwork, under the transform box and the marching ants: a guide is
            // something the picture is aligned against, not something drawn on it. It is the one
            // pass *outside* the paper scissor, because a guide is measured against the view —
            // it runs edge to edge, meeting the ruler it was pulled from, rather than stopping
            // where the paper does (`guide_instances`). Drawn even with the paper fully off
            // screen, which is why it does not sit inside the `if let` either.
            if guide_count > 0 {
                pass.set_scissor_rect(0, 0, self.config.width, self.config.height);
                pass.set_pipeline(&self.guide_pipeline);
                pass.set_bind_group(0, &self.preview_bg, &[]);
                pass.set_vertex_buffer(0, self.guide_buf.slice(..));
                pass.draw(0..6, 0..guide_count);
            }

            if let Some((x, y, w, h)) = scissor {
                pass.set_scissor_rect(x, y, w, h);

                if !overlay_range.is_empty() {
                    pass.set_pipeline(&self.stroke_pipeline);
                    pass.set_bind_group(0, &self.preview_bg, &[]);
                    pass.set_vertex_buffer(0, self.stroke_buf.slice(..));
                    pass.draw(0..6, overlay_range.clone());
                }

                if has_preview_shape {
                    pass.set_pipeline(&self.shape_pipeline);
                    pass.set_bind_group(0, &self.preview_bg, &[]);
                    pass.draw(0..3, 0..1);
                }
            }

            // Crop's rect and the whole-layer transform box are both free to extend past the
            // paper — that is what expanding the canvas and scaling/rotating a layer off it
            // both are — so, like the guide pass above, their chrome (rect outline, handles,
            // composition guides) draws edge to edge instead of clipping at the very boundary
            // the user is dragging it past. Drawn even with the paper scissor empty (paper
            // fully off screen), same as guides. Every other screen-space overlay this
            // pipeline also carries — the hover outline, the text caret — keeps clipping to
            // the paper.
            if !screen_overlay_range.is_empty() {
                let unclipped = doc.tool == Tool::Crop || doc.transform_handles().is_some();
                let crop_scissor =
                    unclipped.then_some((0, 0, self.config.width, self.config.height));
                if let Some((x, y, w, h)) = crop_scissor.or(scissor) {
                    pass.set_scissor_rect(x, y, w, h);
                    pass.set_pipeline(&self.overlay_pipeline);
                    pass.set_bind_group(0, &self.preview_bg, &[]);
                    pass.set_vertex_buffer(0, self.stroke_buf.slice(..));
                    pass.draw(0..6, screen_overlay_range.clone());
                }
            }

            if brush_active {
                pass.set_scissor_rect(0, 0, self.config.width, self.config.height);
                self.stroke_coverage.composite(&mut pass, &self.preview_bg);
            }

            if !brush_ring_range.is_empty() {
                pass.set_scissor_rect(0, 0, self.config.width, self.config.height);
                pass.set_pipeline(&self.overlay_pipeline);
                pass.set_bind_group(0, &self.preview_bg, &[]);
                pass.set_vertex_buffer(0, self.stroke_buf.slice(..));
                pass.draw(0..6, brush_ring_range.clone());
            }
        }
    }
}
