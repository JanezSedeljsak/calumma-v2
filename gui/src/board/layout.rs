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
    pub radius: f32,
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

pub fn hole_in_board(board: &BoardLayout, chrome: BoardRect) -> Option<BoardRect> {
    if chrome.is_empty() {
        return None;
    }
    let board_w = board.width as f32;
    let board_h = board.height as f32;
    let x = chrome.x - board.x;
    let y = chrome.y - board.y;
    if x + chrome.width < 1.0 || y + chrome.height < 1.0 || x > board_w - 1.0 || y > board_h - 1.0 {
        return None;
    }
    // Keep the chrome rect whole rather than trimming it to the board: the mask is clipped
    // to the surface anyway, and a trimmed hole would round off the edge it was trimmed at.
    Some(BoardRect {
        x,
        y,
        width: chrome.width,
        height: chrome.height,
        radius: chrome
            .radius
            .min(chrome.width / 2.0)
            .min(chrome.height / 2.0),
    })
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
        sig = sig
            .wrapping_mul(1_000_003)
            .wrapping_add((hole.radius * 10.0).round() as i32 as u32 as u64);
    }
    sig
}
