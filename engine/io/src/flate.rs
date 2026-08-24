//! `/FlateDecode` for the PDF writer.
//!
//! `flate2` is already in this crate's dependency graph — the `png` crate pulls it in — but
//! does not re-export a general compressor, so it is named directly rather than adding
//! anything genuinely new. PDF's only widely-supported general-purpose filter is zlib, which
//! is why the undo stack's zstd is no help here.
use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::io::Write;

/// Zlib-compress a stream, falling back to storing it uncompressed. The fallback is not
/// theoretical politeness: an uncompressed stream is valid PDF, so a compressor failure
/// should cost bytes rather than the whole export.
pub fn deflate(data: &[u8]) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    if encoder.write_all(data).is_err() {
        return data.to_vec();
    }
    encoder.finish().unwrap_or_else(|_| data.to_vec())
}
