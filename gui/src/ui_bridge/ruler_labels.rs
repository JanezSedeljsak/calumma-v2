use slint::{Image, Rgba8Pixel, SharedPixelBuffer};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

const LABEL_CACHE_LIMIT: usize = 512;

#[derive(Clone)]
pub struct RulerLabel {
    pub image: Image,
    pub width: u32,
    pub height: u32,
    pub ink_x: i32,
    pub ink_y: i32,
}

type LabelKey = (String, u32, [u8; 4], bool);

thread_local! {
    static LABELS: RefCell<HashMap<LabelKey, Option<Rc<RulerLabel>>>> = RefCell::new(HashMap::new());
}

/// One ruler number as device pixels, rasterized once per text, size, colour and direction —
/// panning reuses the same handful of labels, so a camera move almost never rasterizes.
pub fn ruler_label(
    text: &str,
    size_px: f32,
    color: [u8; 4],
    turned: bool,
) -> Option<Rc<RulerLabel>> {
    let key = (text.to_string(), size_px.to_bits(), color, turned);
    LABELS.with(|labels| {
        let mut labels = labels.borrow_mut();
        if let Some(label) = labels.get(&key) {
            return label.clone();
        }
        if labels.len() >= LABEL_CACHE_LIMIT {
            labels.clear();
        }
        let label = calumma_app::label_bitmap(text, size_px, color, turned).map(|bitmap| {
            let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
                &bitmap.rgba,
                bitmap.width,
                bitmap.height,
            );
            Rc::new(RulerLabel {
                image: Image::from_rgba8(buffer),
                width: bitmap.width,
                height: bitmap.height,
                ink_x: bitmap.ink_x,
                ink_y: bitmap.ink_y,
            })
        });
        labels.insert(key, label.clone());
        label
    })
}
