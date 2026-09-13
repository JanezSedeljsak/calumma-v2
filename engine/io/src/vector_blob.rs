use calumma_core::{Shape, Tool, VectorItem, VectorPath, VectorShape};

const VERSION: u32 = 4;

const TAG_PATH: u8 = 0;
const TAG_SHAPE: u8 = 1;

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
    out.extend_from_slice(&(path.ring_starts.len() as u32).to_le_bytes());
    for &start in &path.ring_starts {
        out.extend_from_slice(&start.to_le_bytes());
    }
    out.push(u8::from(path.even_odd));
}

fn decode_path(r: &mut Reader) -> Option<VectorPath> {
    let n = r.u32()? as usize;
    let mut points = Vec::with_capacity(n.min(4096));
    for _ in 0..n {
        points.push(r.point()?);
    }
    let closed = r.bool()?;
    let fill = r.bool()?;
    let color = r.rgba()?;
    let stroke_width = r.f32()?;
    let stroke = r.bool()?;
    let stroke_color = r.rgba()?;
    let ring_count = r.u32()? as usize;
    let mut ring_starts = Vec::with_capacity(ring_count.min(4096));
    for _ in 0..ring_count {
        ring_starts.push(r.u32()?);
    }
    let even_odd = r.bool()?;
    Some(VectorPath {
        points,
        closed,
        fill,
        color,
        stroke,
        stroke_color,
        stroke_width,
        ring_starts,
        even_odd,
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

fn decode_shape(r: &mut Reader) -> Option<VectorShape> {
    let tool = Tool::from_u32(r.u32()?)?;
    let start = r.point()?;
    let end = r.point()?;
    let half_width = r.f32()?;
    let fill = r.bool()?;
    let color = r.rgba()?;
    let stroke = r.bool()?;
    let stroke_color = r.rgba()?;
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

pub fn encode(item: &VectorItem) -> Vec<u8> {
    encode_all(std::slice::from_ref(item))
}

pub(crate) fn encode_all(items: &[VectorItem]) -> Vec<u8> {
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
    if r.u32()? != VERSION {
        return None;
    }
    let count = r.u32()? as usize;
    let mut items = Vec::with_capacity(count.min(4096));
    for _ in 0..count {
        let item = match r.u8()? {
            TAG_PATH => VectorItem::Path(decode_path(&mut r)?),
            TAG_SHAPE => VectorItem::Shape(decode_shape(&mut r)?),
            _ => return None,
        };
        items.push(item);
    }
    Some(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rings_and_the_fill_rule_round_trip() {
        let item = VectorItem::Path(VectorPath {
            points: vec![
                (0.0, 0.0),
                (10.0, 0.0),
                (10.0, 10.0),
                (3.0, 3.0),
                (7.0, 3.0),
                (7.0, 7.0),
            ],
            closed: true,
            fill: true,
            color: [1, 2, 3, 255],
            stroke: false,
            stroke_color: [0, 0, 0, 255],
            stroke_width: 1.0,
            ring_starts: vec![3],
            even_odd: false,
        });
        assert_eq!(decode(&encode(&item)).unwrap(), vec![item]);
    }

    fn sample_path() -> VectorItem {
        VectorItem::Path(VectorPath {
            points: vec![(0.0, 0.0), (10.0, 0.0), (10.0, 5.0), (0.0, 5.0)],
            closed: true,
            fill: true,
            color: [255, 128, 0, 200],
            stroke: true,
            stroke_color: [0, 0, 0, 255],
            stroke_width: 2.5,
            ring_starts: Vec::new(),
            even_odd: true,
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

    #[test]
    fn round_trips_paths_and_shapes_together() {
        let items = vec![sample_path(), sample_shape(), sample_path()];
        assert_eq!(decode(&encode_all(&items)).unwrap(), items);
    }

    #[test]
    fn a_shape_survives_as_a_shape_not_a_polyline() {
        let items = vec![sample_shape()];
        let decoded = decode(&encode_all(&items)).unwrap();
        assert!(matches!(decoded[0], VectorItem::Shape(_)));
    }

    #[test]
    fn empty_round_trips() {
        assert_eq!(decode(&encode_all(&items_none())).unwrap(), Vec::new());
    }

    fn items_none() -> Vec<VectorItem> {
        Vec::new()
    }

    #[test]
    fn a_truncated_blob_is_rejected_not_panicked_on() {
        let bytes = encode_all(&[sample_path(), sample_shape()]);
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
