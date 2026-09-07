#[derive(Clone, Copy, Debug)]
pub struct BoardLayout {
    pub x: f32,
    pub y: f32,
    pub width: u32,
    pub height: u32,
}

pub fn board_layout(x: f32, y: f32, width: f32, height: f32) -> BoardLayout {
    BoardLayout {
        x,
        y,
        width: width.max(1.0) as u32,
        height: height.max(1.0) as u32,
    }
}
