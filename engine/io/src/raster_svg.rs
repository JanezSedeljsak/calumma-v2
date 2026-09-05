use calumma_core::unpremultiply_rgba;
use resvg::{tiny_skia, usvg};

pub fn decode(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let tree = usvg::Tree::from_data(bytes, &usvg::Options::default()).ok()?;
    let size = tree.size();
    let src_w = size.width().max(1.0);
    let src_h = size.height().max(1.0);
    let long = src_w.max(src_h);
    let cap = calumma_core::limits::IMPORT_MAX_SIDE as f32;
    let scale = if long > cap { cap / long } else { 1.0 };
    let width = (src_w * scale).round().max(1.0) as u32;
    let height = (src_h * scale).round().max(1.0) as u32;
    let mut pixmap = tiny_skia::Pixmap::new(width, height)?;
    let transform = tiny_skia::Transform::from_scale(width as f32 / src_w, height as f32 / src_h);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    let mut rgba = pixmap.data().to_vec();
    unpremultiply_rgba(&mut rgba);
    Some((width, height, rgba))
}
