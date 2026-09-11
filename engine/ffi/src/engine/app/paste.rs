use super::Engine;
use anyhow::{bail, Context, Result};
use calumma_core::paste::{PasteImage, PasteOutcome, PasteSource, PasteVector};
use calumma_io::SvgVector;
use rayon::prelude::*;

enum Decoded {
    Raster {
        name: String,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    },
    Vector {
        name: String,
        svg: SvgVector,
    },
}

impl Decoded {
    fn size(&self) -> (u32, u32) {
        match self {
            Self::Raster { width, height, .. } => (*width, *height),
            Self::Vector { svg, .. } => (svg.width, svg.height),
        }
    }

    fn as_source(&self) -> PasteSource<'_> {
        match self {
            Self::Raster {
                name,
                width,
                height,
                rgba,
            } => PasteSource::Raster(PasteImage {
                name,
                rgba,
                width: *width,
                height: *height,
            }),
            Self::Vector { name, svg } => PasteSource::Vector(PasteVector {
                name,
                items: &svg.items,
                width: svg.width,
                height: svg.height,
            }),
        }
    }
}

/// An SVG that converts cleanly becomes vectors; anything else — including an SVG using
/// something a vector path cannot express — goes through the raster decoder. Files decode in
/// parallel and outside the engine lock, because a batch of HEIC or AVIF photos is seconds of
/// work and the board must not freeze behind it.
fn decode_all(images: &[(&str, &[u8])]) -> Vec<Decoded> {
    images
        .par_iter()
        .filter_map(|&(name, bytes)| {
            let name = name.to_string();
            if let Some(svg) = calumma_io::decode_svg_vector(bytes) {
                return Some(Decoded::Vector { name, svg });
            }
            let (width, height, rgba) = calumma_io::decode_encoded(bytes)?;
            Some(Decoded::Raster {
                name,
                width,
                height,
                rgba,
            })
        })
        .collect()
}

fn sources(decoded: &[Decoded]) -> Vec<PasteSource<'_>> {
    decoded.iter().map(Decoded::as_source).collect()
}

impl Engine {
    pub fn paste_encoded_images(&mut self, images: &[(&str, &[u8])]) -> (usize, PasteOutcome) {
        let decoded = decode_all(images);
        if decoded.is_empty() {
            return (0, PasteOutcome::Failed);
        }
        let mut inner = self.inner.lock();
        let result = {
            let Some(doc) = inner.doc.as_mut() else {
                return (0, PasteOutcome::Failed);
            };
            doc.commit_text();
            doc.paste_sources_as_layers(&sources(&decoded))
        };
        if result.0 > 0 {
            inner.dirty_save = true;
            inner.invalidate_renderer();
        }
        result
    }

    pub fn create_project_from_encoded_images(
        &mut self,
        name: &str,
        images: &[(&str, &[u8])],
    ) -> Result<String> {
        let decoded = decode_all(images);
        let (width, height) = decoded.first().context("decoding artwork bytes")?.size();
        let mut inner = self.inner.lock();
        inner.close_document();
        let mut doc = inner
            .store
            .create(name, width, height)
            .with_context(|| format!("creating project {name} at {width}x{height}"))?;
        if doc.install_sources_staggered(&sources(&decoded)) == 0 {
            bail!("placing imported images into the new project");
        }
        inner
            .store
            .save(&mut doc)
            .context("saving imported project")?;
        let id = doc.id.clone();
        inner.install_document(doc);
        Ok(id)
    }
}
