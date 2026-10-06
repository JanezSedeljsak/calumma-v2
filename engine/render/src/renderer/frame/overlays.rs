//! This frame's overlay instances: the shape-preview uniform, ink previews, screen-space chrome
//! and the brush ring, written behind the vector-path prefix of the stroke buffer.

use super::*;
use crate::compose::subject_sweep_instances;

pub(super) const ERASER_PREVIEW_COLOR: [f32; 4] = [0.5, 0.5, 0.5, 0.5];

pub(super) const SELECTION_OUTLINE_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 0.9];

pub(super) const SELECTION_OUTLINE_WIDTH: f32 = 1.5;

pub(super) fn collect_screen_overlays(doc: &Document, elapsed: f32, out: &mut Vec<StrokeInstance>) {
    if doc.text_editing() {
        out.extend(text_overlay_instances(doc, elapsed));
    } else if doc.tool == Tool::Crop {
        out.extend(crop_overlay_instances(doc));
    } else if let Some(handles) = doc.transform_handles() {
        out.extend(transform_overlay_instances(handles));
    }
    out.extend(vector_selection_instances(doc));
    out.extend(clone_source_overlay_instances(doc));
    for (index, corners) in doc.layer_highlights() {
        let covered = doc
            .transform_handles()
            .is_some_and(|(handle_index, _, _)| handle_index == index);
        if !covered {
            out.extend(layer_highlight_instances(corners, doc.camera.zoom));
        }
    }
    if let Some((index, corners)) = doc.background_removal_target() {
        let outlined = doc
            .layer_highlights()
            .iter()
            .any(|(highlighted, _)| *highlighted == index);
        if !outlined {
            out.extend(layer_highlight_instances(corners, doc.camera.zoom));
        }
        out.extend(subject_sweep_instances(corners, elapsed, doc.camera.zoom));
    }
}

impl Renderer {
    /// The shape-preview uniform, written every frame. Returns the shape being dragged out, if
    /// any, which is what decides whether the board pass draws the shape pipeline at all.
    pub(super) fn write_preview_uniforms(
        &mut self,
        doc: &Document,
        viewport: [f32; 2],
        color: [f32; 4],
    ) -> Option<calumma_core::Shape> {
        let preview_shape = doc.preview_shape();
        let (p0, p1, tool, half_width, fill, shape_stroke, shape_color, shape_stroke_color) =
            match preview_shape {
                Some(s) => {
                    let (fill_ink, stroke_ink) = doc.shape_paint(s.tool);
                    (
                        [s.start.0, s.start.1],
                        [s.end.0, s.end.1],
                        s.tool as u32 as f32,
                        s.half_width,
                        f32::from(u8::from(s.fill)),
                        f32::from(u8::from(s.stroke)),
                        rgba_unit(fill_ink),
                        rgba_unit(stroke_ink),
                    )
                }
                None => match selection_rect_or_ellipse(doc) {
                    // The marquee is an outline and nothing else, so it rides in on the stroke
                    // half of the same uniform the shape preview uses.
                    Some((p0, p1, sel_tool)) => (
                        p0,
                        p1,
                        sel_tool as u32 as f32,
                        SELECTION_OUTLINE_WIDTH,
                        0.0,
                        1.0,
                        SELECTION_OUTLINE_COLOR,
                        SELECTION_OUTLINE_COLOR,
                    ),
                    None => ([0.0, 0.0], [0.0, 0.0], 0.0, 0.0, 0.0, 0.0, color, color),
                },
            };
        // Written every frame, not only on the ones that build an overlay: the guide pass reads
        // the camera out of this buffer, and guides are board furniture that has to keep up with
        // a pan the overlay sits out.
        let preview = PreviewUniforms {
            pan: [doc.camera.pan_x, doc.camera.pan_y],
            zoom: doc.camera.zoom,
            dpr: doc.camera.dpr,
            viewport,
            _align_color: [0.0, 0.0],
            color: shape_color,
            p0,
            p1,
            half_width,
            tool,
            fill,
            shape_stroke,
            stroke_ink: rgba_unit(doc.stroke_ink()),
            shape_stroke_color,
        };
        self.queue
            .write_buffer(&self.preview_buf, 0, bytemuck::bytes_of(&preview));
        preview_shape
    }

    /// This frame's overlay instances — ink previews, screen-space chrome, the brush ring and
    /// the live brush segments — written behind the vector-path prefix of the stroke buffer,
    /// with the range each pass draws.
    pub(super) fn build_overlays(
        &mut self,
        doc: &Document,
        plan: FramePlan,
        pan: (f32, f32),
        color: [f32; 4],
    ) -> OverlayRanges {
        let FramePlan {
            camera_only,
            need_draw_rebuild,
            ..
        } = plan;
        let overlay_range;
        let screen_overlay_range;
        let brush_ring_range;
        // `brush_range` is the segments to *union into* the coverage target this frame, which is
        // empty on any frame the pointer did not move; `brush_active` is whether there is a live
        // brush stroke to composite onto the board at all.
        let mut brush_range = 0u32..0u32;
        let mut brush_active = false;
        let mut brush_restart = false;
        if !camera_only {
            let radius = doc.effective_brush_size() * 0.5;
            let stroke_color = if doc.tool == Tool::Eraser {
                ERASER_PREVIEW_COLOR
            } else {
                color
            };
            let mut brush_instances: Vec<StrokeInstance> = Vec::new();
            // The stroke buffer is a vector-path prefix (owned by `cached_draws`' ranges)
            // followed by this frame's overlay. Only the overlay changes on an overlay frame,
            // so it is built into a reused scratch buffer and written at the prefix's offset —
            // cloning `cached_strokes` every frame just to rewrite a suffix put a full copy of
            // every vector path in the document on the hot path.
            //
            // The overlay itself splits in two, by which pass measures it: ink-shaped previews
            // stay in document units on `stroke_pipeline`, while chrome — the transform and
            // item frames, the text session's box and caret, the hover outline — is measured in
            // screen pixels on `overlay_pipeline`. Both are contiguous ranges of the same
            // buffer, so the split costs a second `draw`, not a second upload.
            let prefix_len = if self.camera_motion {
                0
            } else {
                self.cached_strokes.len()
            };
            let mut instances = std::mem::take(&mut self.overlay_scratch);
            instances.clear();
            let mut screen_instances = std::mem::take(&mut self.screen_overlay_scratch);
            screen_instances.clear();
            let overlay_start = prefix_len as u32;
            if doc.previews_brush_stroke() {
                brush_active = true;
                let profile = doc.active_brush_profile();
                let recreated = self.stroke_coverage.ensure(
                    &self.device,
                    self.config.width,
                    self.config.height,
                );
                let progress = CoverageProgress {
                    generation: doc.stroke_generation(),
                    points: doc.stroke_points.len(),
                    pan,
                    zoom: doc.camera.zoom,
                    dpr: doc.camera.dpr,
                    brush: brush_params(radius, &profile),
                    color: stroke_color,
                };
                let first_segment = match self.coverage_progress {
                    Some(prev) if !recreated && prev.appendable(&progress) => {
                        stroke_segment_count(prev.points)
                    }
                    _ => 0,
                };
                brush_restart = first_segment == 0;
                brush_instances = stroke_instances_from(
                    &doc.stroke_points,
                    first_segment,
                    radius,
                    stroke_color,
                    &profile,
                );
                self.coverage_progress = Some(progress);
            } else if !doc.stroke_points.is_empty() && doc.tool.previews_stroke() {
                instances.extend(stroke_instances(
                    &doc.stroke_points,
                    radius,
                    stroke_color,
                    &BrushProfile::HARD,
                ));
            } else if !doc.text_editing()
                && doc.tool != Tool::Crop
                && doc.transform_handles().is_none()
            {
                if let Some(points) = selection_lasso_points(doc) {
                    instances.extend(stroke_instances(
                        &points,
                        SELECTION_OUTLINE_WIDTH,
                        SELECTION_OUTLINE_COLOR,
                        &BrushProfile::HARD,
                    ));
                } else if let Some(edges) =
                    selection_mask_edges(doc, SELECTION_OUTLINE_WIDTH, SELECTION_OUTLINE_COLOR)
                {
                    instances.extend(edges);
                }
            }
            collect_screen_overlays(
                doc,
                self.started.elapsed().as_secs_f32(),
                &mut screen_instances,
            );
            overlay_range = overlay_start..overlay_start + instances.len() as u32;
            let screen_start = overlay_range.end;
            instances.extend_from_slice(&screen_instances);
            self.screen_overlay_scratch = screen_instances;
            screen_overlay_range =
                screen_start..screen_start + self.screen_overlay_scratch.len() as u32;
            let ring = brush_ring_instances(doc);
            let ring_start = screen_overlay_range.end;
            instances.extend_from_slice(&ring);
            brush_ring_range = ring_start..ring_start + ring.len() as u32;
            let brush_start = brush_ring_range.end;
            instances.append(&mut brush_instances);
            brush_range = brush_start..prefix_len as u32 + instances.len() as u32;
            let total = prefix_len + instances.len();
            let grew = self.ensure_stroke_capacity(total);
            let stride = std::mem::size_of::<StrokeInstance>() as u64;
            if (grew || need_draw_rebuild) && prefix_len > 0 {
                self.queue.write_buffer(
                    &self.stroke_buf,
                    0,
                    bytemuck::cast_slice(&self.cached_strokes),
                );
            }
            if !instances.is_empty() {
                self.queue.write_buffer(
                    &self.stroke_buf,
                    prefix_len as u64 * stride,
                    bytemuck::cast_slice(&instances),
                );
            }
            self.last_overlay_range = overlay_range.clone();
            self.screen_overlay_start = screen_start;
            self.overlay_scratch = instances;
        } else {
            overlay_range = self.last_overlay_range.clone();
            let mut screen_instances = std::mem::take(&mut self.screen_overlay_scratch);
            screen_instances.clear();
            collect_screen_overlays(
                doc,
                self.started.elapsed().as_secs_f32(),
                &mut screen_instances,
            );
            let screen_start = if need_draw_rebuild {
                if self.camera_motion {
                    0
                } else {
                    self.cached_strokes.len() as u32
                }
            } else {
                self.screen_overlay_start
            };
            let ring = brush_ring_instances(doc);
            screen_overlay_range = screen_start..screen_start + screen_instances.len() as u32;
            brush_ring_range =
                screen_overlay_range.end..screen_overlay_range.end + ring.len() as u32;
            let mut overlay_tail = screen_instances;
            overlay_tail.extend(ring);
            let total = screen_start as usize + overlay_tail.len();
            let grew = self.ensure_stroke_capacity(total);
            let stride = std::mem::size_of::<StrokeInstance>() as u64;
            if (grew || need_draw_rebuild) && screen_start > 0 && !self.cached_strokes.is_empty() {
                self.queue.write_buffer(
                    &self.stroke_buf,
                    0,
                    bytemuck::cast_slice(&self.cached_strokes),
                );
            }
            if !overlay_tail.is_empty() {
                self.queue.write_buffer(
                    &self.stroke_buf,
                    u64::from(screen_start) * stride,
                    bytemuck::cast_slice(&overlay_tail),
                );
            }
            let screen_len = screen_overlay_range.end - screen_overlay_range.start;
            overlay_tail.truncate(screen_len as usize);
            self.screen_overlay_start = screen_start;
            self.screen_overlay_scratch = overlay_tail;
        }
        OverlayRanges {
            overlay: overlay_range,
            screen: screen_overlay_range,
            brush_ring: brush_ring_range,
            brush: brush_range,
            brush_active,
            brush_restart,
        }
    }
}
