use calumma_core::*;

#[test]
fn pack_and_unpack_round_trip() {
    let rgb = [12u8, 34, 56];
    assert_eq!(unpack_rgb(pack_rgb(rgb)), rgb);
    let rgba = [12u8, 34, 56, 78];
    assert_eq!(unpack_rgba(pack_rgba(rgba)), rgba);
}

#[test]
fn hex_rgb_parses_hash_and_short_forms() {
    assert_eq!(parse_hex_rgb("#1a2b3c"), Some([0x1A, 0x2B, 0x3C]));
    assert_eq!(parse_hex_rgb("1A2B3C"), Some([0x1A, 0x2B, 0x3C]));
    assert_eq!(parse_hex_rgb("abc"), Some([0xAA, 0xBB, 0xCC]));
    assert_eq!(parse_hex_rgb("  #fff  "), Some([255, 255, 255]));
    assert_eq!(parse_hex_rgb("gg0000"), None);
    assert_eq!(parse_hex_rgb("12345"), None);
}

#[test]
fn hex_rgb_formats_uppercase_without_hash() {
    assert_eq!(format_hex_rgb([0x1A, 0x2B, 0x3C]), "1A2B3C");
    assert_eq!(format_hex_rgb([0, 0, 0]), "000000");
}
