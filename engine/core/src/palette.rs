use uuid::Uuid;

pub const PROJECT_COLORS: [[u8; 3]; 10] = [
    [60, 201, 214],
    [240, 148, 74],
    [232, 90, 90],
    [124, 176, 92],
    [138, 132, 226],
    [226, 132, 190],
    [72, 160, 232],
    [216, 186, 78],
    [96, 196, 168],
    [178, 122, 96],
];

pub fn project_color(index: usize) -> [u8; 3] {
    PROJECT_COLORS[index % PROJECT_COLORS.len()]
}

pub fn random_project_color() -> [u8; 3] {
    let byte = Uuid::new_v4().as_bytes()[0] as usize;
    project_color(byte)
}

pub fn color_for_seed(seed: &str) -> [u8; 3] {
    let sum = seed
        .bytes()
        .fold(0usize, |acc, b| acc.wrapping_add(b as usize));
    project_color(sum)
}

/// The desk's squared paper, in screen points. Screen-space rather than document-space on
/// purpose: the pattern is the surface the board sits *on*, so it holds still while the paper
/// pans and zooms over it.
///
/// These live here rather than as literals in `board.wgsl` because the shader is not the only
/// thing that draws them — `CanvasSkeleton` stands in for the board while a project loads, and
/// has to lay the same grid on the same 26pt lattice or the swap is visible. Rust is the one
/// source: the shader reads them out of `PaperUniforms`, the shell reads them over
/// `calm_desk_metrics`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DeskMetrics {
    /// Side of one square.
    pub cell: f32,
    /// Width of the faint rule along each cell edge.
    pub line_width: f32,
    /// Half-length of each arm of the cross sitting on a cell corner.
    pub cross_arm: f32,
    /// Thickness of those arms.
    pub cross_line_width: f32,
}

impl DeskMetrics {
    pub const DEFAULT: Self = Self {
        cell: 26.0,
        line_width: 1.0,
        cross_arm: 3.5,
        cross_line_width: 1.1,
    };

    /// How much of `grid` the cell rules take, against the full-strength crosses. The rules are
    /// the quieter half of the pattern; without this they read as a table rather than as paper.
    pub const LINE_ALPHA: f32 = 0.4;
}

impl Default for DeskMetrics {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoardColors {
    pub desk: [u8; 4],
    pub grid: [u8; 4],
    pub paper_border: [u8; 4],
}

impl BoardColors {
    pub fn fallback(dark: bool) -> Self {
        if dark {
            Self {
                desk: [14, 18, 22, 255],
                grid: [26, 34, 40, 255],
                paper_border: [255, 255, 255, 64],
            }
        } else {
            Self {
                desk: [244, 247, 249, 255],
                grid: [183, 196, 206, 255],
                paper_border: [0, 0, 0, 64],
            }
        }
    }
}

impl Default for BoardColors {
    fn default() -> Self {
        Self::fallback(true)
    }
}
