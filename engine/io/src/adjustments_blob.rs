use calumma_core::Adjustments;

const FIELD_COUNT: usize = 5;
const BLOB_LEN: usize = FIELD_COUNT * 4;

pub fn encode(adj: &Adjustments) -> Vec<u8> {
    let mut out = Vec::with_capacity(BLOB_LEN);
    for value in [
        adj.brightness,
        adj.contrast,
        adj.vibrance,
        adj.saturation,
        adj.levels_gamma,
    ] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

pub fn decode(bytes: &[u8]) -> Option<Adjustments> {
    if bytes.len() != BLOB_LEN {
        return None;
    }
    Some(Adjustments {
        brightness: f32::from_le_bytes(bytes[0..4].try_into().ok()?),
        contrast: f32::from_le_bytes(bytes[4..8].try_into().ok()?),
        vibrance: f32::from_le_bytes(bytes[8..12].try_into().ok()?),
        saturation: f32::from_le_bytes(bytes[12..16].try_into().ok()?),
        levels_gamma: f32::from_le_bytes(bytes[16..20].try_into().ok()?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let adj = Adjustments {
            brightness: 0.2,
            contrast: -0.1,
            vibrance: 0.4,
            saturation: -0.3,
            levels_gamma: 1.2,
        };
        let bytes = encode(&adj);
        assert_eq!(decode(&bytes), Some(adj));
    }

    #[test]
    fn rejects_wrong_length() {
        assert_eq!(decode(&[0u8; 3]), None);
    }

    #[test]
    fn rejects_stale_seven_field_blob() {
        let stale = vec![0u8; 28];
        assert_eq!(decode(&stale), None);
    }
}
