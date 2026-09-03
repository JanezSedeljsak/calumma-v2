use calumma_core::{default_guide_color, Guide, GuideAxis};

/// Leading byte of a blob written since guides gained a color. It is a *version* tag that works
/// without ever having written one before: the first byte of a legacy blob is a `GuideAxis`, so
/// it is 0 or 1, and anything else can only be a tag. Adding a length prefix instead would have
/// been ambiguous — a legacy blob's length is a multiple of 5, and some of those are also a
/// valid tagged length.
const VERSION_COLORED: u8 = 2;

/// `axis` + `f32` position, the shape written before guides had a color. Decoded forever;
/// never written again.
const LEGACY_RECORD_LEN: usize = 5;
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
///
/// A blob saved before guides had a color still decodes, with every guide taking
/// [`default_guide_color`]: that is exactly the color those rules were drawn in when they were
/// written, so an old project opens looking the way it was left.
pub fn decode(bytes: &[u8]) -> Option<Vec<Guide>> {
    match bytes.first() {
        None => Some(Vec::new()),
        Some(&VERSION_COLORED) => decode_colored(&bytes[1..]),
        Some(_) => decode_legacy(bytes),
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

fn decode_legacy(bytes: &[u8]) -> Option<Vec<Guide>> {
    if bytes.len() % LEGACY_RECORD_LEN != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / LEGACY_RECORD_LEN);
    for record in bytes.chunks_exact(LEGACY_RECORD_LEN) {
        out.push(Guide {
            axis: GuideAxis::from_u8(record[0])?,
            position: finite(&record[1..5])?,
            color: default_guide_color(),
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

    /// The legacy shape, written by hand: this is the one thing that cannot be produced by
    /// `encode` any more, and every project saved before guides had a color is one of these.
    fn legacy_bytes(guides: &[(GuideAxis, f32)]) -> Vec<u8> {
        let mut out = Vec::new();
        for &(axis, position) in guides {
            out.push(u8::from(axis));
            out.extend_from_slice(&position.to_le_bytes());
        }
        out
    }

    #[test]
    fn round_trip() {
        assert_eq!(decode(&encode(&sample())), Some(sample()));
    }

    #[test]
    fn empty_round_trips_as_empty() {
        assert_eq!(decode(&encode(&[])), Some(Vec::new()));
    }

    /// A project saved before this feature opens with its rules exactly as they were drawn —
    /// which is the default color, because that is the only one they could have been.
    #[test]
    fn a_blob_from_before_guides_had_a_color_decodes_in_the_default_one() {
        let bytes = legacy_bytes(&[(GuideAxis::Horizontal, 12.5), (GuideAxis::Vertical, 240.0)]);

        let guides = decode(&bytes).expect("legacy blobs stay readable");

        assert_eq!(guides.len(), 2);
        assert_eq!(guides[0].position, 12.5);
        assert_eq!(guides[1].axis, GuideAxis::Vertical);
        assert!(guides.iter().all(|g| g.color == default_guide_color()));
    }

    /// The version tag is only readable because a legacy blob's first byte is an axis. If
    /// `GuideAxis` ever grew a third variant, that variant would collide with the tag and this
    /// is what would say so.
    #[test]
    fn the_version_tag_cannot_be_mistaken_for_an_axis() {
        assert!(GuideAxis::from_u8(VERSION_COLORED).is_none());
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
