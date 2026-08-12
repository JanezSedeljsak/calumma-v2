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
