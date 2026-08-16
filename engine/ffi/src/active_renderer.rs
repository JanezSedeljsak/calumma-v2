use calumma_core::Document;
use calumma_render::Renderer;

#[allow(clippy::large_enum_variant)]
pub(crate) enum ActiveRenderer {
    #[allow(dead_code)]
    Gpu(Renderer),
    #[cfg(test)]
    Stub,
}

impl ActiveRenderer {
    pub(crate) fn resize(&mut self, width: u32, height: u32) {
        match self {
            Self::Gpu(renderer) => renderer.resize(width, height),
            #[cfg(test)]
            Self::Stub => {
                let _ = (width, height);
            }
        }
    }

    pub(crate) fn invalidate(&mut self) {
        match self {
            Self::Gpu(renderer) => renderer.invalidate(),
            #[cfg(test)]
            Self::Stub => {}
        }
    }

    pub(crate) fn invalidate_overlay(&mut self) {
        match self {
            Self::Gpu(renderer) => renderer.invalidate_overlay(),
            #[cfg(test)]
            Self::Stub => {}
        }
    }

    pub(crate) fn invalidate_camera(&mut self) {
        match self {
            Self::Gpu(renderer) => renderer.invalidate_camera(),
            #[cfg(test)]
            Self::Stub => {}
        }
    }

    pub(crate) fn end_camera_motion(&mut self) {
        match self {
            Self::Gpu(renderer) => renderer.end_camera_motion(),
            #[cfg(test)]
            Self::Stub => {}
        }
    }

    pub(crate) fn request_overview_prewarm(&mut self) {
        match self {
            Self::Gpu(renderer) => renderer.request_overview_prewarm(),
            #[cfg(test)]
            Self::Stub => {}
        }
    }

    pub(crate) fn release_document(&mut self) {
        match self {
            Self::Gpu(renderer) => renderer.release_document(),
            #[cfg(test)]
            Self::Stub => {}
        }
    }

    pub(crate) fn gpu_tile_bytes(&self) -> usize {
        match self {
            Self::Gpu(renderer) => renderer.gpu_tile_bytes(),
            #[cfg(test)]
            Self::Stub => 0,
        }
    }

    pub(crate) fn render(&mut self, doc: &mut Document) {
        match self {
            Self::Gpu(renderer) => renderer.render(doc),
            #[cfg(test)]
            Self::Stub => {
                let _ = doc;
            }
        }
    }
}
