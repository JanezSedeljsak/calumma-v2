use calumma_core::{Shape, Tool, VectorItem, VectorPath, VectorShape};

/// v1 stored a flat list of polyline paths, before shapes were kept as parameters. v2 adds
/// a per-item tag so a rect can round-trip as a rect instead of being flattened into four
/// points on the way to disk — which would have thrown away exactly the thing that makes it
/// a vector. v3 splits the single colour and `fill` flag into an independent fill and
/// stroke, so a shape can carry both at once. Older blobs still decode: v1 items come back
/// as `Path`s, and a v2 item's one colour becomes whichever of the two it was being used
/// as — see `legacy_paint`.
const VERSION_PATHS_ONLY: u32 = 1;
const VERSION_ONE_COLOR: u32 = 2;
const VERSION: u32 = 3;

const TAG_PATH: u8 = 0;
const TAG_SHAPE: u8 = 1;

/// A pre-v3 item had one colour and a `fill` flag deciding what it painted. Filled means the
/// colour was the fill and there was no outline; unfilled means it was the outline.
fn legacy_paint(fill: bool, color: [u8; 4]) -> ([u8; 4], bool, [u8; 4]) {
    if fill {
        (color, false, color)
    } else {
        (color, true, color)
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.offset.checked_add(n)?;
        let slice = self.bytes.get(self.offset..end)?;
        self.offset = end;
        Some(slice)
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn f32(&mut self) -> Option<f32> {
        Some(f32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }

    fn bool(&mut self) -> Option<bool> {
        Some(self.u8()? != 0)
    }

    fn rgba(&mut self) -> Option<[u8; 4]> {
        self.take(4)?.try_into().ok()
    }

    fn point(&mut self) -> Option<(f32, f32)> {
        Some((self.f32()?, self.f32()?))
    }
}

fn write_point(out: &mut Vec<u8>, p: (f32, f32)) {
    out.extend_from_slice(&p.0.to_le_bytes());
    out.extend_from_slice(&p.1.to_le_bytes());
}

fn encode_path(out: &mut Vec<u8>, path: &VectorPath) {
    out.extend_from_slice(&(path.points.len() as u32).to_le_bytes());
    for &p in &path.points {
        write_point(out, p);
    }
    out.push(u8::from(path.closed));
    out.push(u8::from(path.fill));
    out.extend_from_slice(&path.color);
    out.extend_from_slice(&path.stroke_width.to_le_bytes());
    out.push(u8::from(path.stroke));
    out.extend_from_slice(&path.stroke_color);
}

fn decode_path(r: &mut Reader, version: u32) -> Option<VectorPath> {
    let n = r.u32()? as usize;
    let mut points = Vec::with_capacity(n.min(4096));
    for _ in 0..n {
        points.push(r.point()?);
    }
    let closed = r.bool()?;
    let fill = r.bool()?;
    let color = r.rgba()?;
    let stroke_width = r.f32()?;
    let (color, stroke, stroke_color) = if version < VERSION {
        legacy_paint(fill && closed, color)
    } else {
        (color, r.bool()?, r.rgba()?)
    };
    Some(VectorPath {
        points,
        closed,
        fill,
        color,
        stroke,
        stroke_color,
        stroke_width,
    })
}

fn encode_shape(out: &mut Vec<u8>, item: &VectorShape) {
    out.extend_from_slice(&(item.shape.tool as u32).to_le_bytes());
    write_point(out, item.shape.start);
    write_point(out, item.shape.end);
    out.extend_from_slice(&item.shape.half_width.to_le_bytes());
    out.push(u8::from(item.shape.fill));
    out.extend_from_slice(&item.color);
    out.push(u8::from(item.shape.stroke));
    out.extend_from_slice(&item.stroke_color);
}

fn decode_shape(r: &mut Reader, version: u32) -> Option<VectorShape> {
    let tool = Tool::from_u32(r.u32()?)?;
    let start = r.point()?;
    let end = r.point()?;
    let half_width = r.f32()?;
    let fill = r.bool()?;
    let color = r.rgba()?;
    let (color, stroke, stroke_color) = if version < VERSION {
        legacy_paint(fill && tool.takes_fill(), color)
    } else {
        (color, r.bool()?, r.rgba()?)
    };
    Some(VectorShape {
        shape: Shape {
            tool,
            start,
            end,
            half_width,
            fill,
            stroke,
        },
        color,
        stroke_color,
    })
}

pub fn encode(items: &[VectorItem]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&(items.len() as u32).to_le_bytes());
    for item in items {
        match item {
            VectorItem::Path(p) => {
                out.push(TAG_PATH);
                encode_path(&mut out, p);
            }
            VectorItem::Shape(s) => {
                out.push(TAG_SHAPE);
                encode_shape(&mut out, s);
            }
        }
    }
    out
}

pub fn decode(bytes: &[u8]) -> Option<Vec<VectorItem>> {
    let mut r = Reader::new(bytes);
    let version = r.u32()?;
    let count = r.u32()? as usize;
    let mut items = Vec::with_capacity(count.min(4096));
    for _ in 0..count {
        let item = match version {
            VERSION_PATHS_ONLY => VectorItem::Path(decode_path(&mut r, version)?),
            VERSION_ONE_COLOR | VERSION => match r.u8()? {
                TAG_PATH => VectorItem::Path(decode_path(&mut r, version)?),
                TAG_SHAPE => VectorItem::Shape(decode_shape(&mut r, version)?),
                _ => return None,
            },
            _ => return None,
        };
        items.push(item);
    }
    Some(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_path() -> VectorItem {
        VectorItem::Path(VectorPath {
            points: vec![(0.0, 0.0), (10.0, 0.0), (10.0, 5.0), (0.0, 5.0)],
            closed: true,
            fill: true,
            color: [255, 128, 0, 200],
            stroke: true,
            stroke_color: [0, 0, 0, 255],
            stroke_width: 2.5,
        })
    }

    fn sample_shape() -> VectorItem {
        VectorItem::Shape(VectorShape {
            shape: Shape {
                tool: Tool::Ellipse,
                start: (4.0, 8.0),
                end: (40.0, 24.0),
                half_width: 1.5,
                fill: true,
                stroke: true,
            },
            color: [10, 20, 30, 255],
            stroke_color: [200, 200, 200, 255],
        })
    }

    /// A path written the way v1 and v2 wrote one: one colour, no stroke fields.
    fn encode_legacy_path(out: &mut Vec<u8>, path: &VectorPath) {
        out.extend_from_slice(&(path.points.len() as u32).to_le_bytes());
        for &p in &path.points {
            write_point(out, p);
        }
        out.push(u8::from(path.closed));
        out.push(u8::from(path.fill));
        out.extend_from_slice(&path.color);
        out.extend_from_slice(&path.stroke_width.to_le_bytes());
    }

    fn encode_legacy_shape(out: &mut Vec<u8>, item: &VectorShape) {
        out.extend_from_slice(&(item.shape.tool as u32).to_le_bytes());
        write_point(out, item.shape.start);
        write_point(out, item.shape.end);
        out.extend_from_slice(&item.shape.half_width.to_le_bytes());
        out.push(u8::from(item.shape.fill));
        out.extend_from_slice(&item.color);
    }

    #[test]
    fn round_trips_paths_and_shapes_together() {
        let items = vec![sample_path(), sample_shape(), sample_path()];
        assert_eq!(decode(&encode(&items)).unwrap(), items);
    }

    #[test]
    fn a_shape_survives_as_a_shape_not_a_polyline() {
        let items = vec![sample_shape()];
        let decoded = decode(&encode(&items)).unwrap();
        assert!(matches!(decoded[0], VectorItem::Shape(_)));
    }

    #[test]
    fn empty_round_trips() {
        assert_eq!(decode(&encode(&items_none())).unwrap(), Vec::new());
    }

    fn items_none() -> Vec<VectorItem> {
        Vec::new()
    }

    #[test]
    fn a_v1_blob_still_decodes_as_paths() {
        let mut legacy = Vec::new();
        legacy.extend_from_slice(&VERSION_PATHS_ONLY.to_le_bytes());
        legacy.extend_from_slice(&1u32.to_le_bytes());
        let VectorItem::Path(mut path) = sample_path() else {
            unreachable!()
        };
        encode_legacy_path(&mut legacy, &path);
        // A v1 filled closed path had no outline, so that is what it comes back as.
        path.stroke = false;
        path.stroke_color = path.color;
        assert_eq!(decode(&legacy).unwrap(), vec![VectorItem::Path(path)]);
    }

    #[test]
    fn a_v2_filled_shape_keeps_its_colour_as_the_fill_and_gains_no_outline() {
        let mut legacy = Vec::new();
        legacy.extend_from_slice(&VERSION_ONE_COLOR.to_le_bytes());
        legacy.extend_from_slice(&1u32.to_le_bytes());
        legacy.push(TAG_SHAPE);
        let VectorItem::Shape(mut shape) = sample_shape() else {
            unreachable!()
        };
        encode_legacy_shape(&mut legacy, &shape);
        shape.shape.stroke = false;
        shape.stroke_color = shape.color;
        assert_eq!(decode(&legacy).unwrap(), vec![VectorItem::Shape(shape)]);
    }

    #[test]
    fn a_v2_outlined_shape_comes_back_as_a_stroke_in_the_same_colour() {
        let mut legacy = Vec::new();
        legacy.extend_from_slice(&VERSION_ONE_COLOR.to_le_bytes());
        legacy.extend_from_slice(&1u32.to_le_bytes());
        legacy.push(TAG_SHAPE);
        let VectorItem::Shape(mut shape) = sample_shape() else {
            unreachable!()
        };
        shape.shape.fill = false;
        encode_legacy_shape(&mut legacy, &shape);
        shape.stroke_color = shape.color;
        let decoded = decode(&legacy).unwrap();
        assert_eq!(decoded, vec![VectorItem::Shape(shape)]);
    }

    #[test]
    fn a_truncated_blob_is_rejected_not_panicked_on() {
        let bytes = encode(&[sample_path(), sample_shape()]);
        for cut in 1..bytes.len() {
            let _ = decode(&bytes[..cut]);
        }
        assert!(decode(&bytes[..bytes.len() - 1]).is_none());
    }

    #[test]
    fn an_unknown_version_is_rejected() {
        let mut bytes = 99u32.to_le_bytes().to_vec();
        bytes.extend_from_slice(&1u32.to_le_bytes());
        assert!(decode(&bytes).is_none());
    }
}
