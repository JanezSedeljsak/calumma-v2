use calumma_core::LayerTransform;

const FIELD_COUNT: usize = 5;
const BLOB_LEN: usize = FIELD_COUNT * 4;

pub fn encode(t: &LayerTransform) -> Vec<u8> {
    let mut out = Vec::with_capacity(BLOB_LEN);
    for value in [t.offset_x, t.offset_y, t.scale_x, t.scale_y, t.rotation] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

pub fn decode(bytes: &[u8]) -> Option<LayerTransform> {
    if bytes.len() != BLOB_LEN {
        return None;
    }
    Some(LayerTransform {
        offset_x: f32::from_le_bytes(bytes[0..4].try_into().ok()?),
        offset_y: f32::from_le_bytes(bytes[4..8].try_into().ok()?),
        scale_x: f32::from_le_bytes(bytes[8..12].try_into().ok()?),
        scale_y: f32::from_le_bytes(bytes[12..16].try_into().ok()?),
        rotation: f32::from_le_bytes(bytes[16..20].try_into().ok()?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let t = LayerTransform {
            offset_x: 12.5,
            offset_y: -8.0,
            scale_x: 1.4,
            scale_y: 0.75,
            rotation: 0.6,
        };
        let bytes = encode(&t);
        assert_eq!(decode(&bytes), Some(t));
    }

    #[test]
    fn rejects_wrong_length() {
        assert_eq!(decode(&[0u8; 3]), None);
    }
}
