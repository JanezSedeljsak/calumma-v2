use calumma_core::{Guide, GuideAxis};

const VERSION_COLORED: u8 = 2;

/// `axis` + `f32` position + RGB.
const RECORD_LEN: usize = 8;

pub fn encode(guides: &[Guide]) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 + guides.len() * RECORD_LEN);
    out.push(VERSION_COLORED);
    for guide in guides {
        out.push(u8::from(guide.axis));
        out.extend_from_slice(&guide.position.to_le_bytes());
        out.extend_from_slice(&guide.color);
    }
    out
}

/// `None` for anything that is not a whole number of well-formed records — a blob written by a
/// newer build, or a damaged one, costs the project its guides and nothing else.
pub fn decode(bytes: &[u8]) -> Option<Vec<Guide>> {
    match bytes.first() {
        None => Some(Vec::new()),
        Some(&VERSION_COLORED) => decode_colored(&bytes[1..]),
        Some(_) => None,
    }
}

fn decode_colored(bytes: &[u8]) -> Option<Vec<Guide>> {
    if bytes.len() % RECORD_LEN != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / RECORD_LEN);
    for record in bytes.chunks_exact(RECORD_LEN) {
        out.push(Guide {
            axis: GuideAxis::from_u8(record[0])?,
            position: finite(&record[1..5])?,
            color: [record[5], record[6], record[7]],
        });
    }
    Some(out)
}

fn finite(bytes: &[u8]) -> Option<f32> {
    let position = f32::from_le_bytes(bytes.try_into().ok()?);
    position.is_finite().then_some(position)
}

#[cfg(test)]
mod tests {
    use super::*;
    use calumma_core::default_guide_color;

    fn sample() -> Vec<Guide> {
        vec![
            Guide {
                axis: GuideAxis::Horizontal,
                position: 12.5,
                color: [12, 200, 90],
            },
            Guide {
                axis: GuideAxis::Vertical,
                position: -8.0,
                color: default_guide_color(),
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
        assert_eq!(decode(&[VERSION_COLORED, 0, 1, 2]), None);
        assert_eq!(decode(&[0, 1, 2]), None);
    }

    #[test]
    fn rejects_an_unknown_axis() {
        let mut bytes = encode(&sample());
        bytes[1] = 7;
        assert_eq!(decode(&bytes), None);
    }

    #[test]
    fn rejects_a_non_finite_position() {
        let bytes = encode(&[Guide {
            axis: GuideAxis::Vertical,
            position: f32::NAN,
            color: [1, 2, 3],
        }]);
        assert_eq!(decode(&bytes), None);
    }
}
