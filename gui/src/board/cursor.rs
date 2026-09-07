use calumma_app::Engine;
use calumma_core::{guide::GuideAxis, Tool};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoardCursor {
    Default,
    OpenHand,
    ClosedHand,
    ZoomIn,
    IBeam,
    ResizeVertical,
    ResizeHorizontal,
    BrushRing,
    Tool(Tool),
    Crosshair,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ModifierState {
    pub space_held: bool,
    pub meta_held: bool,
    pub alt_held: bool,
    pub shift_held: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BoardCursorInput {
    pub pointer_inside: bool,
    pub panning: bool,
    pub painting: bool,
    pub modal_open: bool,
    pub hover_x: f32,
    pub hover_y: f32,
    pub mods: ModifierState,
}

pub fn pan_chord(mods: ModifierState, tool: Tool) -> bool {
    if mods.alt_held && !mods.meta_held && matches!(tool, Tool::Clone | Tool::Heal) {
        return false;
    }
    mods.meta_held || mods.alt_held
}

pub fn should_pan(mods: ModifierState, tool: Tool) -> bool {
    mods.space_held || pan_chord(mods, tool)
}

pub fn should_track_hover(input: &BoardCursorInput, tool: Tool) -> bool {
    !input.panning
        && !input.painting
        && !input.mods.space_held
        && !pan_chord(input.mods, tool)
        && !input.modal_open
}

pub fn pick_cursor(engine: &Engine, input: &BoardCursorInput) -> BoardCursor {
    if input.modal_open {
        return BoardCursor::Default;
    }
    if !input.pointer_inside && !input.panning && !input.painting {
        return BoardCursor::Default;
    }
    if input.panning {
        return BoardCursor::ClosedHand;
    }
    if input.mods.space_held {
        return BoardCursor::OpenHand;
    }
    if pan_chord(input.mods, engine.active_tool().unwrap_or(Tool::Pen)) {
        return BoardCursor::ZoomIn;
    }
    let tool = engine.active_tool().unwrap_or(Tool::Pen);
    if tool == Tool::Text {
        return BoardCursor::IBeam;
    }
    if tool == Tool::Move {
        return match engine.guide_axis_at(input.hover_x, input.hover_y) {
            Some(GuideAxis::Horizontal) => BoardCursor::ResizeVertical,
            Some(GuideAxis::Vertical) => BoardCursor::ResizeHorizontal,
            None => BoardCursor::Default,
        };
    }
    if engine.brush_ring_visible() {
        return BoardCursor::BrushRing;
    }
    if tool_cursor_icon(tool).is_some() {
        return BoardCursor::Tool(tool);
    }
    BoardCursor::Crosshair
}

pub fn tool_cursor_icon(tool: Tool) -> Option<&'static str> {
    if matches!(tool, Tool::Text | Tool::Move) {
        return None;
    }
    Some(if tool.is_selection() {
        match tool {
            Tool::SelectEllipse => "select-ellipse",
            Tool::SelectLasso => "select-lasso",
            Tool::MagicWand => "magic-wand",
            Tool::SelectColor => "select-color",
            _ => "select-rect",
        }
    } else {
        match tool {
            Tool::Pen => "pen",
            Tool::Eraser => "eraser",
            Tool::Blur => "blur",
            Tool::Clone => "clone",
            Tool::Heal => "heal",
            Tool::Fill => "bucket",
            Tool::Eyedropper => "eyedropper",
            Tool::Transform => "transform",
            Tool::Line => "line",
            Tool::Rect => "shape",
            Tool::Ellipse => "ellipse",
            Tool::Arrow => "arrow",
            Tool::Triangle => "triangle",
            Tool::Pentagon => "pentagon",
            Tool::Crop => "crop",
            _ => return None,
        }
    })
}

pub struct CursorController {
    last: Option<BoardCursor>,
    icons_root: PathBuf,
    #[cfg(target_os = "macos")]
    cache: cursor_macos::Cache,
}

impl CursorController {
    pub fn new(icons_root: PathBuf) -> Self {
        Self {
            last: None,
            icons_root,
            #[cfg(target_os = "macos")]
            cache: cursor_macos::Cache::new(),
        }
    }

    pub fn refresh(&mut self, engine: &Engine, input: &BoardCursorInput) {
        let choice = pick_cursor(engine, input);
        if self.last == Some(choice) {
            return;
        }
        self.last = Some(choice);
        #[cfg(target_os = "macos")]
        cursor_macos::apply(choice, &self.icons_root, &mut self.cache);
    }

    pub fn reset(&mut self) {
        self.last = None;
        #[cfg(target_os = "macos")]
        cursor_macos::apply(BoardCursor::Default, &self.icons_root, &mut self.cache);
    }
}

#[cfg(target_os = "macos")]
mod cursor_macos {
    use super::{tool_cursor_icon, BoardCursor};
    use objc2::rc::Retained;
    use objc2::AnyThread;
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSBitmapImageRep, NSCursor, NSDeviceRGBColorSpace, NSImage};
    use objc2_foundation::{NSPoint, NSSize};
    use std::collections::HashMap;
    use std::path::Path;
    use tiny_skia::{Pixmap, Transform};

    const SIDE: f32 = 26.0;
    const HOTSPOT: (f32, f32) = (5.0, 5.0);
    const ARM_LENGTH: f32 = 5.0;
    const ARM_GAP: f32 = 1.5;
    const GLYPH_SIDE: f32 = 15.0;

    pub struct Cache {
        ring: Retained<NSCursor>,
        tools: HashMap<u32, Retained<NSCursor>>,
        glyphs: HashMap<String, Pixmap>,
    }

    impl Cache {
        pub fn new() -> Self {
            Self {
                ring: blank_ring(),
                tools: HashMap::new(),
                glyphs: HashMap::new(),
            }
        }
    }

    pub fn apply(choice: BoardCursor, icons_root: &Path, cache: &mut Cache) {
        let _mtm = MainThreadMarker::new().expect("cursor updates run on the main thread");
        let cursor = match choice {
            BoardCursor::Default => NSCursor::arrowCursor(),
            BoardCursor::OpenHand => NSCursor::openHandCursor(),
            BoardCursor::ClosedHand => NSCursor::closedHandCursor(),
            BoardCursor::ZoomIn => NSCursor::zoomInCursor(),
            BoardCursor::IBeam => NSCursor::IBeamCursor(),
            BoardCursor::ResizeVertical => NSCursor::rowResizeCursor(),
            BoardCursor::ResizeHorizontal => NSCursor::columnResizeCursor(),
            BoardCursor::BrushRing => cache.ring.clone(),
            BoardCursor::Crosshair => NSCursor::crosshairCursor(),
            BoardCursor::Tool(tool) => {
                let key = tool as u32;
                if let Some(cursor) = cache.tools.get(&key) {
                    cursor.clone()
                } else {
                    let name = tool_cursor_icon(tool).unwrap_or("pen");
                    let rgba = build_tool_rgba(cache, name, icons_root);
                    let cursor = rgba_cursor(&rgba, SIDE as u16, SIDE as u16, HOTSPOT);
                    cache.tools.insert(key, cursor.clone());
                    cursor
                }
            }
        };
        cursor.set();
    }

    fn build_tool_rgba(cache: &mut Cache, name: &str, icons_root: &Path) -> Vec<u8> {
        let mut pixmap = Pixmap::new(SIDE as u32, SIDE as u32).expect("cursor pixmap");
        pixmap.fill(tiny_skia::Color::TRANSPARENT);
        for spread in [1.0_f32, 0.0] {
            let color = if spread > 0.0 {
                tiny_skia::Color::from_rgba8(0, 0, 0, 140)
            } else {
                tiny_skia::Color::from_rgba8(255, 255, 255, 255)
            };
            draw_crosshair(&mut pixmap, spread, color);
            if let Some(glyph) = load_glyph(cache, name, icons_root) {
                draw_glyph(&mut pixmap, &glyph, spread, color);
            }
        }
        pixmap.data().to_vec()
    }

    fn draw_crosshair(pixmap: &mut Pixmap, spread: f32, color: tiny_skia::Color) {
        let paint = tiny_skia::Paint {
            anti_alias: false,
            force_hq_pipeline: false,
            shader: tiny_skia::Shader::SolidColor(color),
            blend_mode: tiny_skia::BlendMode::SourceOver,
        };
        let thickness = 1.0 + spread * 2.0;
        let (hx, hy) = HOTSPOT;
        for (dx, dy) in [(-1.0_f32, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
            let horizontal = dy == 0.0;
            let start = ARM_GAP;
            let end = ARM_LENGTH + spread;
            let (x, y, w, h) = if horizontal {
                (
                    hx + if dx < 0.0 { -end } else { start },
                    hy - thickness / 2.0,
                    end - start,
                    thickness,
                )
            } else {
                (
                    hx - thickness / 2.0,
                    hy + if dy < 0.0 { -end } else { start },
                    thickness,
                    end - start,
                )
            };
            pixmap.fill_rect(
                tiny_skia::Rect::from_xywh(x, y, w, h).unwrap(),
                &paint,
                Transform::identity(),
                None,
            );
        }
    }

    fn draw_glyph(pixmap: &mut Pixmap, glyph: &Pixmap, spread: f32, color: tiny_skia::Color) {
        let gx = HOTSPOT.0 + 4.0;
        let gy = SIDE - GLYPH_SIDE;
        let offsets: &[(f32, f32)] = if spread > 0.0 {
            &[
                (-1.0, 0.0),
                (1.0, 0.0),
                (0.0, -1.0),
                (0.0, 1.0),
                (-1.0, -1.0),
                (-1.0, 1.0),
                (1.0, -1.0),
                (1.0, 1.0),
            ]
        } else {
            &[(0.0, 0.0)]
        };
        for (dx, dy) in offsets {
            let mut tinted = glyph.clone();
            tint_glyph(&mut tinted, color);
            pixmap.draw_pixmap(
                (gx + dx * spread).round() as i32,
                (gy + dy * spread).round() as i32,
                tinted.as_ref(),
                &tiny_skia::PixmapPaint::default(),
                Transform::identity(),
                None,
            );
        }
    }

    fn tint_glyph(pixmap: &mut Pixmap, color: tiny_skia::Color) {
        for px in pixmap.pixels_mut() {
            if px.alpha() > 0 {
                *px = tiny_skia::ColorU8::from_rgba(
                    (color.red() * 255.0) as u8,
                    (color.green() * 255.0) as u8,
                    (color.blue() * 255.0) as u8,
                    px.alpha(),
                )
                .premultiply();
            }
        }
    }

    fn load_glyph(cache: &mut Cache, name: &str, icons_root: &Path) -> Option<Pixmap> {
        if let Some(pixmap) = cache.glyphs.get(name) {
            return Some(pixmap.clone());
        }
        let path = icons_root.join(format!("{name}.svg"));
        let pixmap = rasterize_svg(&path, GLYPH_SIDE as u32)?;
        cache.glyphs.insert(name.to_string(), pixmap.clone());
        Some(pixmap)
    }

    fn rasterize_svg(path: &Path, size: u32) -> Option<Pixmap> {
        let data = std::fs::read(path).ok()?;
        let tree = usvg::Tree::from_data(&data, &usvg::Options::default()).ok()?;
        let mut pixmap = Pixmap::new(size, size)?;
        let scale = size as f32 / tree.size().width().max(tree.size().height());
        let transform = Transform::from_scale(scale, scale);
        resvg::render(&tree, transform, &mut pixmap.as_mut());
        Some(pixmap)
    }

    fn blank_ring() -> Retained<NSCursor> {
        rgba_cursor(&[0, 0, 0, 0], 1, 1, (0.0, 0.0))
    }

    fn rgba_cursor(
        rgba: &[u8],
        width: u16,
        height: u16,
        hotspot: (f32, f32),
    ) -> Retained<NSCursor> {
        let mtm = MainThreadMarker::new().expect("custom cursor on main thread");
        let _ = mtm;
        let rep = unsafe {
            NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
                NSBitmapImageRep::alloc(),
                std::ptr::null_mut(),
                width as isize,
                height as isize,
                8,
                4,
                true,
                false,
                NSDeviceRGBColorSpace,
                (width as isize) * 4,
                32,
            )
        }
        .expect("bitmap cursor rep");
        let planes = rep.bitmapData();
        let len = width as usize * height as usize * 4;
        unsafe {
            std::ptr::copy_nonoverlapping(rgba.as_ptr(), planes, len.min(rgba.len()));
        }
        let image =
            NSImage::initWithSize(NSImage::alloc(), NSSize::new(width as f64, height as f64));
        image.addRepresentation(&rep);
        NSCursor::initWithImage_hotSpot(
            NSCursor::alloc(),
            &image,
            NSPoint::new(hotspot.0 as f64, hotspot.1 as f64),
        )
    }
}
