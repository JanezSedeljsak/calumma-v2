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
                desk: [26, 32, 36, 255],
                grid: [60, 71, 80, 255],
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
