use calumma_core::VectorPath;

const VERSION: u32 = 1;

pub fn encode(paths: &[VectorPath]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&(paths.len() as u32).to_le_bytes());
    for path in paths {
        out.extend_from_slice(&(path.points.len() as u32).to_le_bytes());
        for &(x, y) in &path.points {
            out.extend_from_slice(&x.to_le_bytes());
            out.extend_from_slice(&y.to_le_bytes());
        }
        out.push(u8::from(path.closed));
        out.push(u8::from(path.fill));
        out.extend_from_slice(&path.color);
        out.extend_from_slice(&path.stroke_width.to_le_bytes());
    }
    out
}

pub fn decode(bytes: &[u8]) -> Option<Vec<VectorPath>> {
    if bytes.len() < 8 {
        return None;
    }
    let version = u32::from_le_bytes(bytes[0..4].try_into().ok()?);
    if version != VERSION {
        return None;
    }
    let count = u32::from_le_bytes(bytes[4..8].try_into().ok()?) as usize;
    let mut offset = 8;
    let mut paths = Vec::with_capacity(count);
    for _ in 0..count {
        if offset + 4 > bytes.len() {
            return None;
        }
        let n = u32::from_le_bytes(bytes[offset..offset + 4].try_into().ok()?) as usize;
        offset += 4;
        let need = n
            .checked_mul(8)?
            .checked_add(2)?
            .checked_add(4)?
            .checked_add(4)?;
        if offset + need > bytes.len() {
            return None;
        }
        let mut points = Vec::with_capacity(n);
        for _ in 0..n {
            let x = f32::from_le_bytes(bytes[offset..offset + 4].try_into().ok()?);
            offset += 4;
            let y = f32::from_le_bytes(bytes[offset..offset + 4].try_into().ok()?);
            offset += 4;
            points.push((x, y));
        }
        let closed = bytes[offset] != 0;
        offset += 1;
        let fill = bytes[offset] != 0;
        offset += 1;
        let color = [
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ];
        offset += 4;
        let stroke_width = f32::from_le_bytes(bytes[offset..offset + 4].try_into().ok()?);
        offset += 4;
        paths.push(VectorPath {
            points,
            closed,
            fill,
            color,
            stroke_width,
        });
    }
    Some(paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_paths() {
        let paths = vec![VectorPath {
            points: vec![(0.0, 0.0), (10.0, 0.0), (10.0, 5.0), (0.0, 5.0)],
            closed: true,
            fill: true,
            color: [255, 255, 255, 255],
            stroke_width: 0.0,
        }];
        let bytes = encode(&paths);
        let decoded = decode(&bytes).unwrap();
        assert_eq!(decoded, paths);
    }
}
