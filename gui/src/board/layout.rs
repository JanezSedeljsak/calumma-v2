#[derive(Clone, Copy, Debug)]
pub struct BoardLayout {
    pub x: f32,
    pub y: f32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BoardRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl BoardRect {
    pub fn is_empty(self) -> bool {
        self.width < 1.0 || self.height < 1.0
    }
}

pub fn board_layout(x: f32, y: f32, width: f32, height: f32) -> BoardLayout {
    BoardLayout {
        x,
        y,
        width: width.max(1.0) as u32,
        height: height.max(1.0) as u32,
    }
}

pub fn hole_in_board(board: &BoardLayout, chrome: BoardRect, pad: f32) -> Option<BoardRect> {
    if chrome.is_empty() {
        return None;
    }
    let board_w = board.width as f32;
    let board_h = board.height as f32;
    let x0 = (chrome.x - pad).max(board.x);
    let y0 = (chrome.y - pad).max(board.y);
    let x1 = (chrome.x + chrome.width + pad).min(board.x + board_w);
    let y1 = (chrome.y + chrome.height + pad).min(board.y + board_h);
    let width = x1 - x0;
    let height = y1 - y0;
    if width < 1.0 || height < 1.0 {
        None
    } else {
        Some(BoardRect {
            x: x0 - board.x,
            y: y0 - board.y,
            width,
            height,
        })
    }
}

pub fn holes_signature(holes: &[BoardRect]) -> u64 {
    let mut sig = holes.len() as u64;
    for hole in holes {
        sig = sig
            .wrapping_mul(1_000_003)
            .wrapping_add((hole.x * 10.0).round() as i32 as u32 as u64);
        sig = sig
            .wrapping_mul(1_000_003)
            .wrapping_add((hole.y * 10.0).round() as i32 as u32 as u64);
        sig = sig
            .wrapping_mul(1_000_003)
            .wrapping_add((hole.width * 10.0).round() as i32 as u32 as u64);
        sig = sig
            .wrapping_mul(1_000_003)
            .wrapping_add((hole.height * 10.0).round() as i32 as u32 as u64);
    }
    sig
}
