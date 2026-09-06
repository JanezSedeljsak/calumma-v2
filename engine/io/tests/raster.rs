use calumma_core::Document;
use calumma_io::{decode_encoded, encode_psd, encode_rgba, RasterFormat};

fn solid(width: u32, height: u32, rgba: [u8; 4]) -> Vec<u8> {
    rgba.repeat((width * height) as usize)
}

fn roundtrip(format: RasterFormat) -> (u32, u32, Vec<u8>) {
    let width = 8u32;
    let height = 6u32;
    let src = solid(width, height, [200, 40, 40, 255]);
    let encoded = encode_rgba(&src, width, height, format).expect("encode");
    decode_encoded(&encoded).expect("decode")
}

#[test]
fn png_round_trips_lossless() {
    let (w, h, rgba) = roundtrip(RasterFormat::Png);
    assert_eq!((w, h), (8, 6));
    assert_eq!(&rgba[0..4], &[200, 40, 40, 255]);
}

#[test]
fn jpeg_round_trips_the_colour() {
    let (_, _, rgba) = roundtrip(RasterFormat::Jpeg);
    assert!(rgba[0] > 150);
    assert!(rgba[1] < 80);
    assert!(rgba[2] < 80);
    assert_eq!(rgba[3], 255);
}

#[test]
fn webp_round_trips_lossless() {
    let (_, _, rgba) = roundtrip(RasterFormat::Webp);
    assert_eq!(&rgba[0..4], &[200, 40, 40, 255]);
}

#[test]
fn avif_round_trips_the_colour() {
    let (w, h, rgba) = roundtrip(RasterFormat::Avif);
    assert_eq!((w, h), (8, 6));
    assert!(rgba[0] > 140);
    assert!(rgba[1] < 90);
    assert_eq!(rgba[3], 255);
}

#[test]
fn heic_round_trips_the_colour() {
    let (w, h, rgba) = roundtrip(RasterFormat::Heic);
    assert_eq!((w, h), (8, 6));
    assert!(rgba[0] > 140);
    assert!(rgba[1] < 90);
    assert_eq!(rgba[3], 255);
}

#[test]
fn svg_rasterizes_a_filled_rect() {
    let svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"8\" height=\"8\">\
<rect width=\"8\" height=\"8\" fill=\"#c82828\"/></svg>";
    let (w, h, rgba) = decode_encoded(svg).expect("svg");
    assert_eq!((w, h), (8, 8));
    assert!(rgba[0] > 150);
    assert!(rgba[1] < 80);
    assert_eq!(rgba[3], 255);
}

#[test]
fn psd_composite_round_trips() {
    let mut doc = Document::new("p".into(), "t", 8, 8);
    assert!(doc.place_image(&solid(8, 8, [10, 20, 30, 255]), 8, 8));
    let bytes = encode_psd(&doc);
    let (w, h, rgba) = decode_encoded(&bytes).expect("psd");
    assert_eq!((w, h), (8, 8));
    assert_eq!(&rgba[0..4], &[10, 20, 30, 255]);
}

/// `raster.rs::decode_native` tries `psd::decode_flat` (the layered decoder) first and only
/// falls back to `raster_psd::decode` when that refuses — today that means a grayscale PSD,
/// since the layered decoder only understands RGB. This is the one thing `raster_psd.rs`'s own
/// unit tests can't prove: that the fallback is actually wired into the public entry point a
/// real import goes through, not just correct in isolation.
#[test]
fn a_grayscale_psd_falls_back_to_the_flat_decoder() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"8BPS");
    bytes.extend_from_slice(&1u16.to_be_bytes()); // version
    bytes.extend_from_slice(&[0u8; 6]);
    bytes.extend_from_slice(&1u16.to_be_bytes()); // channels
    bytes.extend_from_slice(&2u32.to_be_bytes()); // height
    bytes.extend_from_slice(&2u32.to_be_bytes()); // width
    bytes.extend_from_slice(&8u16.to_be_bytes()); // depth
    bytes.extend_from_slice(&1u16.to_be_bytes()); // mode: grayscale — the layered decoder refuses this
    bytes.extend_from_slice(&0u32.to_be_bytes()); // color mode data
    bytes.extend_from_slice(&0u32.to_be_bytes()); // image resources
    bytes.extend_from_slice(&0u32.to_be_bytes()); // layer & mask info (no layers at all)
    bytes.extend_from_slice(&0u16.to_be_bytes()); // compression: raw
    bytes.extend_from_slice(&[10, 20, 30, 40]); // 2x2 grayscale plane

    let (w, h, rgba) = decode_encoded(&bytes).expect("the flat decoder opens it");
    assert_eq!((w, h), (2, 2));
    assert_eq!(&rgba[0..4], &[10, 10, 10, 255]);
    assert_eq!(&rgba[12..16], &[40, 40, 40, 255]);
}

#[test]
fn garbage_is_rejected() {
    assert!(decode_encoded(&[0, 1, 2, 3, 4]).is_none());
}

#[test]
fn an_oversized_import_is_fitted_to_the_cap() {
    let width = calumma_core::IMPORT_MAX_SIDE + 16;
    let src = solid(width, 4, [255, 0, 0, 255]);
    let png = encode_rgba(&src, width, 4, RasterFormat::Png).expect("png");
    let (w, h, _) = decode_encoded(&png).expect("decode");
    assert!(w <= calumma_core::IMPORT_MAX_SIDE);
    assert!(h <= calumma_core::IMPORT_MAX_SIDE);
    assert!(w >= h);
}
