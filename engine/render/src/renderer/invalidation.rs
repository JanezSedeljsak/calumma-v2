use super::*;

impl Renderer {
    pub fn invalidate(&mut self) {
        self.frame_dirty = FrameDirty::Content;
        self.visible_upload_needed = None;
        self.cached_retained_span = None;
        self.cached_visible_span = None;
        self.cached_tile_draw_count = None;
        self.clear_layer_cache();
        self.overview.mark_dirty();
        self.pan_cache.invalidate();
    }

    /// Cheapest invalidation there is: redraw with fresh overlays, keep every cache. Never
    /// downgrades a pending `Camera` or `Content` frame — those already imply an overlay pass.
    pub fn invalidate_overlay(&mut self) {
        if self.frame_dirty == FrameDirty::Clean {
            self.frame_dirty = FrameDirty::Overlay;
        }
    }

    pub fn invalidate_camera(&mut self) {
        self.begin_camera_motion();
        if self.frame_dirty != FrameDirty::Content {
            self.frame_dirty = FrameDirty::Camera;
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        if self.config.width != width || self.config.height != height {
            self.config.width = width;
            self.config.height = height;
            self.output.configure(&self.device, &self.config);
            self.invalidate_camera();
        }
    }

    /// Grows the instance buffer if it has to, reporting whether it did. A reallocation
    /// discards the contents, so the caller has to rewrite the vector-path prefix it would
    /// otherwise have left in place.
    pub(super) fn ensure_stroke_capacity(&mut self, count: usize) -> bool {
        if count <= self.stroke_capacity {
            return false;
        }
        let next = count.next_power_of_two().max(STROKE_INSTANCE_CAPACITY);
        self.stroke_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("stroke-instances"),
            size: (next * std::mem::size_of::<StrokeInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.stroke_capacity = next;
        true
    }

    /// Rebuilds and uploads the guide instances, returning how many there are. Guides are
    /// their own tiny buffer rather than a slice of the overlay's on purpose: the overlay is
    /// skipped on a camera-only frame, and a rule that vanished every time the board was
    /// panned would not be a rule.
    pub(super) fn write_guides(&mut self, doc: &Document) -> u32 {
        let mut guides = std::mem::take(&mut self.guide_scratch);
        guides.clear();
        guides.extend(guide_instances(doc));
        if !guides.is_empty() {
            self.queue
                .write_buffer(&self.guide_buf, 0, bytemuck::cast_slice(&guides));
        }
        let count = guides.len() as u32;
        self.guide_scratch = guides;
        count
    }

    pub(super) fn ensure_vector_shape_capacity(&mut self, count: usize) {
        if count <= self.vector_shape_capacity {
            return;
        }
        let next = count
            .next_power_of_two()
            .max(VECTOR_SHAPE_INSTANCE_CAPACITY);
        self.vector_shape_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vector-shape-instances"),
            size: (next * std::mem::size_of::<VectorShapeInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.vector_shape_capacity = next;
    }

    pub(super) fn ensure_tile_instance_capacity(&mut self, count: usize) {
        if count <= self.tile_instance_capacity {
            return;
        }
        let next = count.next_power_of_two().max(TILE_INSTANCE_CAPACITY);
        self.tile_instance_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tile-instances"),
            size: (next * std::mem::size_of::<TileInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.tile_instance_capacity = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn progress(generation: u64, points: usize) -> CoverageProgress {
        CoverageProgress {
            generation,
            points,
            pan: (0.0, 0.0),
            zoom: 1.0,
            dpr: 2.0,
            brush: [8.0, 1.0, 0.0, 1.0],
            color: [0.1, 0.1, 0.1, 1.0],
        }
    }

    /// The ordinary frame: the pointer travelled, the camera did not, so the segments already
    /// unioned into the coverage target stay and only the new ones are drawn.
    #[test]
    fn a_stroke_that_only_grew_is_appended_to() {
        assert!(progress(7, 40).appendable(&progress(7, 41)));
        assert!(progress(7, 40).appendable(&progress(7, 40)));
    }

    /// `stroke_segment_count` maps one point to one segment — the degenerate capsule behind a
    /// tap's dot — and segment 0 replaces it rather than following it, so the first real capsule
    /// would never be drawn if this appended.
    #[test]
    fn the_one_point_dot_restarts_rather_than_being_appended_to() {
        assert!(!progress(7, 1).appendable(&progress(7, 2)));
        assert!(!progress(7, 0).appendable(&progress(7, 1)));
    }

    /// Two guards on the same failure, because `Max` blending cannot take a capsule back out of
    /// the target. `Document::push_stroke_point` bumps the generation when a Shift-held straight
    /// segment rewinds the list; the shorter-point-count test is what catches a rewind that
    /// forgot to.
    #[test]
    fn a_rewound_or_restarted_stroke_is_not_appendable() {
        assert!(!progress(7, 40).appendable(&progress(8, 41)));
        assert!(!progress(7, 40).appendable(&progress(7, 39)));
    }

    /// The coverage pass rasterizes in device pixels off the preview uniform, so pixels
    /// accumulated at one camera are in the wrong place at the next one — and a capsule carries
    /// the width and color it was drawn at.
    #[test]
    fn moving_the_camera_or_the_brush_invalidates_what_was_accumulated() {
        let base = progress(7, 40);
        let mut panned = progress(7, 41);
        panned.pan = (4.0, 0.0);
        assert!(!base.appendable(&panned));

        let mut zoomed = progress(7, 41);
        zoomed.zoom = 1.5;
        assert!(!base.appendable(&zoomed));

        let mut rescaled = progress(7, 41);
        rescaled.dpr = 1.0;
        assert!(!base.appendable(&rescaled));

        let mut resized = progress(7, 41);
        resized.brush[0] = 12.0;
        assert!(!base.appendable(&resized));

        let mut recolored = progress(7, 41);
        recolored.color = [1.0, 0.0, 0.0, 1.0];
        assert!(!base.appendable(&recolored));
    }

    fn renderer() -> Option<Renderer> {
        Renderer::new_headless(64, 64)
    }

    fn doc() -> Document {
        let mut doc = Document::new("p".into(), "t", 128, 128);
        doc.resize_viewport(64.0, 64.0, 1.0);
        doc.fit_to_view();
        doc
    }

    /// The sledgehammer: every cache that lets a still frame skip work goes back to "ask
    /// again", and the next frame is a full `Content` rebuild no matter what was pending.
    #[test]
    fn invalidate_clears_every_frame_cache_and_forces_a_content_frame() {
        let Some(mut r) = renderer() else { return };
        let mut d = doc();
        r.render(&mut d);
        assert!(
            r.cached_retained_span.is_some(),
            "a render populates the span caches"
        );
        assert!(r.cached_visible_span.is_some());
        r.visible_upload_needed = Some(true);

        r.invalidate();

        assert_eq!(r.frame_dirty, FrameDirty::Content);
        assert_eq!(r.cached_retained_span, None);
        assert_eq!(r.cached_visible_span, None);
        assert_eq!(r.cached_tile_draw_count, None);
        assert_eq!(r.visible_upload_needed, None);
    }

    /// The one invariant the doc comment promises: an overlay-only invalidation is the
    /// cheapest there is, so it must never step on a frame that already implies more work.
    #[test]
    fn invalidate_overlay_never_downgrades_a_pending_camera_or_content_frame() {
        let Some(mut r) = renderer() else { return };

        r.frame_dirty = FrameDirty::Clean;
        r.invalidate_overlay();
        assert_eq!(
            r.frame_dirty,
            FrameDirty::Overlay,
            "clean is free to move to overlay"
        );

        r.frame_dirty = FrameDirty::Camera;
        r.invalidate_overlay();
        assert_eq!(
            r.frame_dirty,
            FrameDirty::Camera,
            "camera already implies an overlay pass"
        );

        r.frame_dirty = FrameDirty::Content;
        r.invalidate_overlay();
        assert_eq!(
            r.frame_dirty,
            FrameDirty::Content,
            "content is never downgraded either"
        );
    }

    /// `invalidate_camera` always starts camera motion, but it only escalates `frame_dirty` to
    /// `Camera` — a pending `Content` frame (a paint stroke mid-gesture, say) needs its full
    /// rebuild and must not be quietly narrowed to a camera-only redraw.
    #[test]
    fn invalidate_camera_begins_motion_but_never_downgrades_content() {
        let Some(mut r) = renderer() else { return };

        r.frame_dirty = FrameDirty::Clean;
        r.invalidate_camera();
        assert!(r.camera_motion);
        assert_eq!(r.frame_dirty, FrameDirty::Camera);

        r.camera_motion = false;
        r.frame_dirty = FrameDirty::Content;
        r.invalidate_camera();
        assert!(r.camera_motion, "motion still begins");
        assert_eq!(
            r.frame_dirty,
            FrameDirty::Content,
            "but content is not narrowed to camera"
        );
    }

    /// A zero dimension is a transient the shell can report mid-layout, not a real target size —
    /// resizing to it would configure a surface no draw can complete against.
    #[test]
    fn resize_to_a_zero_dimension_is_ignored() {
        let Some(mut r) = renderer() else { return };
        r.frame_dirty = FrameDirty::Clean;

        r.resize(0, 200);
        assert_eq!((r.config.width, r.config.height), (64, 64));
        assert_eq!(r.frame_dirty, FrameDirty::Clean);

        r.resize(200, 0);
        assert_eq!((r.config.width, r.config.height), (64, 64));
        assert_eq!(r.frame_dirty, FrameDirty::Clean);
    }

    /// Resizing to the size it already is must not reconfigure the surface or invalidate the
    /// camera — otherwise every layout pass that happens to report the same size would cost a
    /// full redraw for nothing.
    #[test]
    fn resize_to_the_current_size_is_a_no_op() {
        let Some(mut r) = renderer() else { return };
        r.frame_dirty = FrameDirty::Clean;

        r.resize(64, 64);

        assert_eq!(r.frame_dirty, FrameDirty::Clean);
    }

    /// A genuine size change reconfigures the surface and is exactly a camera invalidation —
    /// the picture is the same document at a different viewport, not new content.
    #[test]
    fn resize_to_a_new_size_reconfigures_and_invalidates_the_camera() {
        let Some(mut r) = renderer() else { return };
        r.frame_dirty = FrameDirty::Clean;

        r.resize(128, 96);

        assert_eq!((r.config.width, r.config.height), (128, 96));
        assert!(r.camera_motion);
        assert_eq!(r.frame_dirty, FrameDirty::Camera);
    }

    /// Growth is to the next power of two and never below the baseline capacity, and a request
    /// that already fits must report no growth — the caller uses the return value to decide
    /// whether it has to rewrite the buffer's contents.
    #[test]
    fn ensure_stroke_capacity_grows_by_power_of_two_and_reports_whether_it_did() {
        let Some(mut r) = renderer() else { return };
        assert_eq!(r.stroke_capacity, STROKE_INSTANCE_CAPACITY);

        assert!(!r.ensure_stroke_capacity(STROKE_INSTANCE_CAPACITY));
        assert_eq!(r.stroke_capacity, STROKE_INSTANCE_CAPACITY);

        assert!(r.ensure_stroke_capacity(STROKE_INSTANCE_CAPACITY + 1));
        assert_eq!(
            r.stroke_capacity,
            (STROKE_INSTANCE_CAPACITY + 1).next_power_of_two()
        );

        let grown = r.stroke_capacity;
        assert!(
            !r.ensure_stroke_capacity(grown - 1),
            "already fits, no further growth"
        );
    }

    #[test]
    fn ensure_vector_shape_capacity_grows_the_same_way() {
        let Some(mut r) = renderer() else { return };
        assert_eq!(r.vector_shape_capacity, VECTOR_SHAPE_INSTANCE_CAPACITY);

        r.ensure_vector_shape_capacity(VECTOR_SHAPE_INSTANCE_CAPACITY);
        assert_eq!(r.vector_shape_capacity, VECTOR_SHAPE_INSTANCE_CAPACITY);

        r.ensure_vector_shape_capacity(VECTOR_SHAPE_INSTANCE_CAPACITY * 3);
        assert_eq!(
            r.vector_shape_capacity,
            (VECTOR_SHAPE_INSTANCE_CAPACITY * 3).next_power_of_two()
        );
    }

    #[test]
    fn ensure_tile_instance_capacity_grows_the_same_way() {
        let Some(mut r) = renderer() else { return };
        assert_eq!(r.tile_instance_capacity, TILE_INSTANCE_CAPACITY);

        r.ensure_tile_instance_capacity(TILE_INSTANCE_CAPACITY);
        assert_eq!(r.tile_instance_capacity, TILE_INSTANCE_CAPACITY);

        r.ensure_tile_instance_capacity(TILE_INSTANCE_CAPACITY + 1);
        assert_eq!(
            r.tile_instance_capacity,
            (TILE_INSTANCE_CAPACITY + 1).next_power_of_two()
        );
    }

    /// One instance per guide, capped wherever `Document::add_guide` already caps the list —
    /// `write_guides` has no cap logic of its own to get wrong.
    #[test]
    fn write_guides_returns_one_instance_per_guide() {
        let Some(mut r) = renderer() else { return };
        let mut d = doc();
        assert_eq!(r.write_guides(&d), 0);

        d.add_guide(calumma_core::GuideAxis::Horizontal, 10.0);
        d.add_guide(calumma_core::GuideAxis::Vertical, 20.0);
        d.add_guide(calumma_core::GuideAxis::Vertical, 30.0);

        assert_eq!(r.write_guides(&d), 3);
    }
}
