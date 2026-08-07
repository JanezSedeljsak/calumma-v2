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
                desk: [12, 15, 17, 255],
                grid: [24, 30, 35, 255],
                paper_border: [255, 255, 255, 64],
            }
        } else {
            Self {
                desk: [221, 229, 235, 255],
                grid: [194, 207, 217, 255],
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_color_wraps_the_palette() {
        assert_eq!(project_color(0), PROJECT_COLORS[0]);
        assert_eq!(project_color(PROJECT_COLORS.len()), PROJECT_COLORS[0]);
        assert_eq!(project_color(PROJECT_COLORS.len() + 3), PROJECT_COLORS[3]);
    }

    #[test]
    fn random_project_color_is_always_from_the_palette() {
        for _ in 0..64 {
            assert!(PROJECT_COLORS.contains(&random_project_color()));
        }
    }

    #[test]
    fn color_for_seed_is_stable_and_in_palette() {
        let first = color_for_seed("project-a");
        assert_eq!(first, color_for_seed("project-a"));
        assert!(PROJECT_COLORS.contains(&first));
    }

    #[test]
    fn fallback_board_is_darker_on_dark_theme() {
        let dark = BoardColors::fallback(true);
        let light = BoardColors::fallback(false);
        assert!(dark.desk[0] < light.desk[0]);
        assert_eq!(dark.paper_border[0], 255);
        assert_eq!(light.paper_border[0], 0);
    }
}
