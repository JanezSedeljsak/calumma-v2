pub fn pack_rgb(rgb: [u8; 3]) -> u32 {
    ((rgb[0] as u32) << 16) | ((rgb[1] as u32) << 8) | rgb[2] as u32
}

pub fn unpack_rgb(packed: u32) -> [u8; 3] {
    [
        ((packed >> 16) & 0xFF) as u8,
        ((packed >> 8) & 0xFF) as u8,
        (packed & 0xFF) as u8,
    ]
}

pub fn pack_rgba(rgba: [u8; 4]) -> u32 {
    ((rgba[0] as u32) << 24) | ((rgba[1] as u32) << 16) | ((rgba[2] as u32) << 8) | rgba[3] as u32
}

pub fn unpack_rgba(packed: u32) -> [u8; 4] {
    [
        ((packed >> 24) & 0xFF) as u8,
        ((packed >> 16) & 0xFF) as u8,
        ((packed >> 8) & 0xFF) as u8,
        (packed & 0xFF) as u8,
    ]
}

pub fn format_hex_rgb(rgb: [u8; 3]) -> String {
    format!("{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
}

pub fn parse_hex_rgb(s: &str) -> Option<[u8; 3]> {
    let cleaned = s.trim().trim_start_matches('#');
    let expanded = match cleaned.len() {
        3 => {
            let bytes = cleaned.as_bytes();
            let mut out = String::with_capacity(6);
            for b in bytes {
                out.push(*b as char);
                out.push(*b as char);
            }
            out
        }
        6 => cleaned.to_string(),
        _ => return None,
    };
    if !expanded.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let value = u32::from_str_radix(&expanded, 16).ok()?;
    Some(unpack_rgb(value))
}
