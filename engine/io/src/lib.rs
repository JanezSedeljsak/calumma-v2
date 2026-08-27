mod adjustments_blob;
mod flate;
mod guides_blob;
mod pdf;
mod png;
mod psd;
mod store;
mod svg;
mod text_blob;
mod transform_blob;
mod vector_blob;

pub use png::{decode_png_rgba, encode_png_rgba};
pub use store::{ProjectListItem, ProjectStore, StoreError};
pub use svg::encode_svg;

pub use pdf::{page_size as pdf_page_size, PDF_DEFAULT_DPI};

pub fn encode_psd(doc: &calumma_core::Document) -> Vec<u8> {
    psd::encode(doc)
}

pub fn encode_pdf(doc: &calumma_core::Document, dpi: f32) -> Vec<u8> {
    pdf::encode(doc, dpi)
}
