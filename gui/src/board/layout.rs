#[derive(Clone, Copy, Debug)]
pub struct BoardLayout {
    pub x: f32,
    pub y: f32,
    pub width: u32,
    pub height: u32,
}

pub const EDITOR_PADDING: f32 = 8.0;
pub const TOOLS_PANEL_WIDTH: f32 = 148.0;
pub const LAYERS_PANEL_WIDTH: f32 = 240.0;
pub const LAYERS_RAIL_WIDTH: f32 = 24.0;
pub const MENU_BAR_HEIGHT: f32 = 24.0;

pub fn board_layout(window_w: f32, window_h: f32, layers_open: bool) -> BoardLayout {
    let layers = if layers_open {
        LAYERS_PANEL_WIDTH
    } else {
        LAYERS_RAIL_WIDTH
    };
    let x = EDITOR_PADDING + TOOLS_PANEL_WIDTH + EDITOR_PADDING;
    let y = MENU_BAR_HEIGHT + EDITOR_PADDING;
    let w = window_w - x - EDITOR_PADDING - layers - EDITOR_PADDING;
    let h = window_h - MENU_BAR_HEIGHT - EDITOR_PADDING * 2.0;
    BoardLayout {
        x,
        y,
        width: w.max(1.0) as u32,
        height: h.max(1.0) as u32,
    }
}
