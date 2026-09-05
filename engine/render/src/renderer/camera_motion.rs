use super::*;

impl Renderer {
    pub fn begin_camera_motion(&mut self) {
        self.motion_idle_frames = 0;
        self.camera_motion = true;
    }

    pub fn end_camera_motion(&mut self) {
        if !self.camera_motion {
            return;
        }
        self.camera_motion = false;
        self.motion_idle_frames = 0;
        self.cached_tile_draw_count = None;
        // Anything uploaded mid-gesture is still missing its mip chain. Ask for one more
        // content frame so `sync_tiles` can finish those tiles now that there is idle time.
        if !self.base_only_tiles.is_empty() {
            self.frame_dirty = FrameDirty::Content;
        }
    }

    pub(super) fn tick_camera_motion(&mut self) {
        if !self.camera_motion {
            return;
        }
        self.motion_idle_frames += 1;
        if self.motion_idle_frames >= CAMERA_MOTION_IDLE_FRAMES {
            self.end_camera_motion();
        }
    }

    pub(super) fn visible_span(doc: &Document) -> Option<(i32, i32, i32, i32)> {
        doc.visible_rect().map(|visible| visible.tile_span())
    }

    /// How often the board wants to be drawn from here, in frames per second, or
    /// [`FRAME_HINT_DISPLAY_MAX`] for "as fast as the display allows".
    ///
    /// Read once per frame by the shell, which owns nothing but the ceiling. Everything that
    /// makes this answer `DISPLAY_MAX` is a thing already in flight — a gesture, a camera still
    /// settling, a text session, or a frame the renderer has already marked dirty for itself.
    /// Anything else is a board sitting still, where the display link is the only cost left and
    /// there is no picture waiting on it.
    pub fn frame_hint(&self, doc: &Document) -> u32 {
        if self.camera_motion
            || self.frame_dirty != FrameDirty::Clean
            || doc.has_live_preview()
            || doc.has_animated_overlay()
        {
            return self.budget.frame_hint_ceiling();
        }
        FRAME_HINT_IDLE_FPS
    }

    pub fn request_overview_prewarm(&mut self) {
        self.overview.request_prewarm();
    }

    /// Takes the margin as a parameter, rather than reading `self.budget` directly, so
    /// it stays a pure function of `(doc, margin)` — callers thread the effective level's margin
    /// through, and tests can exercise it without a real `wgpu::Surface`-backed `Renderer`.
    pub(super) fn retained_span(doc: &Document, margin: i32) -> Option<(i32, i32, i32, i32)> {
        doc.visible_rect()
            .map(|visible| visible.expanded_by_tiles(margin).tile_span())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use calumma_core::tile::TILE_SIZE;

    fn doc_with_viewport() -> Document {
        let mut doc = Document::new("p".into(), "t", 2048, 2048);
        doc.resize_viewport(800.0, 600.0, 1.0);
        doc.fit_to_view();
        doc
    }

    #[test]
    fn a_document_with_no_viewport_has_nothing_visible_to_upload_or_retain() {
        let doc = Document::new("p".into(), "t", 512, 512);

        assert_eq!(Renderer::visible_span(&doc), None);
        assert_eq!(
            Renderer::retained_span(&doc, MemoryPressureLevel::Normal.retention_margin_tiles()),
            None
        );
    }

    /// The retained span is the visible one plus a margin, and that margin is the whole
    /// eviction policy: tiles just outside the viewport are kept so a pan does not have to
    /// re-upload the row it is about to reach.
    #[test]
    fn the_retained_span_is_the_visible_span_grown_by_exactly_the_margin() {
        let doc = doc_with_viewport();

        let margin = MemoryPressureLevel::Normal.retention_margin_tiles();
        let (vx0, vy0, vx1, vy1) = Renderer::visible_span(&doc).expect("visible");
        let (rx0, ry0, rx1, ry1) = Renderer::retained_span(&doc, margin).expect("retained");

        assert_eq!((rx0, ry0), (vx0 - margin, vy0 - margin));
        assert_eq!((rx1, ry1), (vx1 + margin, vy1 + margin));
    }

    /// Zoomed in far enough that the paper is larger than the viewport, panning slides the
    /// span across the document — which is what decides the tiles the next frame has to have
    /// resident.
    #[test]
    fn panning_slides_the_visible_span_across_the_document() {
        let mut doc = doc_with_viewport();
        doc.camera.zoom = 2.0;
        doc.camera.pan_x = 0.0;
        doc.camera.pan_y = 0.0;
        let before = Renderer::visible_span(&doc).expect("visible");

        doc.camera.pan_x -= (TILE_SIZE as f32) * 4.0;
        let after = Renderer::visible_span(&doc).expect("visible");

        assert!(after.0 > before.0, "{before:?} -> {after:?}");
        assert!(after.2 > before.2, "{before:?} -> {after:?}");
        assert_eq!(after.1, before.1, "a horizontal pan leaves the rows alone");
        assert_eq!(after.3, before.3);

        doc.camera.pan_x += (TILE_SIZE as f32) * 4.0;
        assert_eq!(
            Renderer::visible_span(&doc),
            Some(before),
            "panning back lands on the same tiles, so a pan gesture uploads nothing new"
        );
    }

    fn renderer() -> Option<Renderer> {
        Renderer::new_headless(64, 64)
    }

    #[test]
    fn begin_camera_motion_starts_the_gesture_and_resets_the_idle_counter() {
        let Some(mut r) = renderer() else { return };
        r.motion_idle_frames = 3;
        r.camera_motion = false;

        r.begin_camera_motion();

        assert!(r.camera_motion);
        assert_eq!(r.motion_idle_frames, 0);
    }

    /// Ending motion that never began is a no-op — the shell may call it defensively without
    /// first checking whether a gesture was in flight.
    #[test]
    fn end_camera_motion_on_an_already_settled_camera_does_nothing() {
        let Some(mut r) = renderer() else { return };
        r.camera_motion = false;
        r.motion_idle_frames = 5;
        r.frame_dirty = FrameDirty::Clean;

        r.end_camera_motion();

        assert_eq!(r.motion_idle_frames, 5, "untouched — nothing was ending");
        assert_eq!(r.frame_dirty, FrameDirty::Clean);
    }

    /// A gesture that never uploaded a base-only tile ends quietly. One that did left a tile
    /// still missing its mip chain, so ending motion has to ask for one more `Content` frame —
    /// that is the only chance `sync_tiles` gets to finish it now that idle time exists.
    #[test]
    fn end_camera_motion_asks_for_a_content_frame_only_if_a_tile_was_left_base_only() {
        let Some(mut r) = renderer() else { return };
        r.begin_camera_motion();
        r.frame_dirty = FrameDirty::Clean;
        r.end_camera_motion();
        assert!(!r.camera_motion);
        assert_eq!(
            r.frame_dirty,
            FrameDirty::Clean,
            "nothing was left base-only, so ending motion asks for nothing extra"
        );

        r.begin_camera_motion();
        r.base_only_tiles.insert((0, 0, 0));
        r.frame_dirty = FrameDirty::Clean;
        r.end_camera_motion();
        assert!(!r.camera_motion);
        assert_eq!(r.cached_tile_draw_count, None);
        assert_eq!(
            r.frame_dirty,
            FrameDirty::Content,
            "a base-only tile needs its mip chain finished"
        );
    }

    /// The countdown that turns a settled camera back into a still board: motion only ends once
    /// `CAMERA_MOTION_IDLE_FRAMES` consecutive ticks have passed with nothing restarting it.
    #[test]
    fn tick_camera_motion_ends_the_gesture_after_the_idle_threshold() {
        let Some(mut r) = renderer() else { return };
        r.begin_camera_motion();

        for _ in 0..CAMERA_MOTION_IDLE_FRAMES - 1 {
            r.tick_camera_motion();
            assert!(r.camera_motion, "not idle long enough yet");
        }
        r.tick_camera_motion();
        assert!(!r.camera_motion, "idle threshold reached");
    }

    #[test]
    fn tick_camera_motion_does_nothing_while_the_camera_is_already_settled() {
        let Some(mut r) = renderer() else { return };
        r.camera_motion = false;
        r.motion_idle_frames = 0;

        r.tick_camera_motion();

        assert_eq!(r.motion_idle_frames, 0);
        assert!(!r.camera_motion);
    }

    /// `frame_hint` is the one thing the shell reads to throttle the display link, so its three
    /// branches — a gesture in flight, a frame already asking to be drawn, and everything else —
    /// have to land on the ceiling, the ceiling, and the idle rate respectively.
    #[test]
    fn frame_hint_asks_for_the_ceiling_whenever_there_is_work_pending() {
        let Some(mut r) = renderer() else { return };
        let mut d = Document::new("p".into(), "t", 64, 64);
        d.resize_viewport(64.0, 64.0, 1.0);

        r.camera_motion = false;
        r.frame_dirty = FrameDirty::Clean;
        assert_eq!(
            r.frame_hint(&d),
            FRAME_HINT_IDLE_FPS,
            "nothing pending — the idle rate is enough"
        );

        r.camera_motion = true;
        assert_eq!(r.frame_hint(&d), r.budget.frame_hint_ceiling());

        r.camera_motion = false;
        r.frame_dirty = FrameDirty::Content;
        assert_eq!(r.frame_hint(&d), r.budget.frame_hint_ceiling());
    }
}
