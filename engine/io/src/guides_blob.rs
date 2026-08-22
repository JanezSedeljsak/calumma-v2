use calumma_core::{Guide, GuideAxis};

const RECORD_LEN: usize = 5;

pub fn encode(guides: &[Guide]) -> Vec<u8> {
    let mut out = Vec::with_capacity(guides.len() * RECORD_LEN);
    for guide in guides {
        out.push(u8::from(guide.axis));
        out.extend_from_slice(&guide.position.to_le_bytes());
    }
    out
}

/// `None` for anything that is not a whole number of well-formed records — a blob written by a
/// newer build, or a damaged one, costs the project its guides and nothing else.
pub fn decode(bytes: &[u8]) -> Option<Vec<Guide>> {
    if bytes.len() % RECORD_LEN != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / RECORD_LEN);
    for record in bytes.chunks_exact(RECORD_LEN) {
        let axis = GuideAxis::from_u8(record[0])?;
        let position = f32::from_le_bytes(record[1..5].try_into().ok()?);
        if !position.is_finite() {
            return None;
        }
        out.push(Guide { axis, position });
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<Guide> {
        vec![
            Guide {
                axis: GuideAxis::Horizontal,
                position: 12.5,
            },
            Guide {
                axis: GuideAxis::Vertical,
                position: -8.0,
            },
        ]
    }

    #[test]
    fn round_trip() {
        assert_eq!(decode(&encode(&sample())), Some(sample()));
    }

    #[test]
    fn empty_round_trips_as_empty() {
        assert_eq!(decode(&encode(&[])), Some(Vec::new()));
    }

    #[test]
    fn rejects_a_partial_record() {
        assert_eq!(decode(&[0, 1, 2]), None);
    }

    #[test]
    fn rejects_an_unknown_axis() {
        let mut bytes = encode(&sample());
        bytes[0] = 7;
        assert_eq!(decode(&bytes), None);
    }

    #[test]
    fn rejects_a_non_finite_position() {
        let bytes = encode(&[Guide {
            axis: GuideAxis::Vertical,
            position: f32::NAN,
        }]);
        assert_eq!(decode(&bytes), None);
    }
}
