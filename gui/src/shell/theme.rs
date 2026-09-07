use anyhow::{Context, Result};
use calumma_core::BoardColors;
use serde::Deserialize;
use slint::Color;
use std::fs;
use std::path::Path;

#[derive(Clone, Debug)]
pub struct Theme {
    pub bg: Color,
    pub surface: Color,
    pub surface_hover: Color,
    pub text: Color,
    pub text_muted: Color,
    pub accent_teal: Color,
    pub accent_orange: Color,
    pub island_border: Color,
    pub control_border: Color,
    pub control_focus_border: Color,
    pub danger: Color,
    pub desk: Color,
    pub desk_grid: Color,
    pub paper: Color,
    pub paper_border: Color,
    pub metrics: Metrics,
    pub presets: Vec<ResolutionPreset>,
}

#[derive(Clone, Debug)]
pub struct ResolutionPreset {
    pub label: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct Metrics {
    pub radius_sm: f32,
    pub radius_md: f32,
    pub radius_lg: f32,
    pub radius_window: f32,
    pub radius_island: f32,
    pub space_xs: f32,
    pub space_sm: f32,
    pub space_md: f32,
    pub space_lg: f32,
    pub space_xl: f32,
    pub space_xxl: f32,
    pub control_height: f32,
    pub label_size: f32,
    pub label_tracking: f32,
    pub body_size: f32,
    pub title_size: f32,
    pub brand_size: f32,
    pub main_min_width: f32,
    pub main_min_height: f32,
    pub new_project_width: f32,
    pub new_project_height: f32,
    pub paste_min_width: f32,
    pub paste_max_width: f32,
    pub paste_min_height: f32,
    pub paste_width_ratio: f32,
}

#[derive(Deserialize)]
struct TokensFile {
    presets: Vec<PresetEntry>,
    color: ColorModes,
    window: WindowTokens,
    radius: RadiusTokens,
    space: SpaceTokens,
    control: ControlTokens,
    #[serde(rename = "type")]
    type_scale: TypeTokens,
}

#[derive(Deserialize)]
#[allow(non_snake_case)]
struct WindowTokens {
    mainWidth: u32,
    mainHeight: u32,
    mainMinWidth: u32,
    mainMinHeight: u32,
    newProjectWidth: f32,
    newProjectHeight: f32,
    pasteMinWidth: f32,
    pasteMaxWidth: f32,
    pasteMinHeight: f32,
    pasteWidthRatio: f32,
}

#[derive(Deserialize)]
struct RadiusTokens {
    sm: f32,
    md: f32,
    lg: f32,
    window: f32,
    island: f32,
}

#[derive(Deserialize)]
struct SpaceTokens {
    xs: f32,
    sm: f32,
    md: f32,
    lg: f32,
    xl: f32,
    xxl: f32,
}

#[derive(Deserialize)]
struct ControlTokens {
    height: f32,
}

#[derive(Deserialize)]
#[allow(non_snake_case)]
struct TypeTokens {
    labelSize: f32,
    labelTracking: f32,
    bodySize: f32,
    titleSize: f32,
    brandSize: f32,
}

#[derive(Clone, Debug)]
pub struct WindowMetrics {
    pub width: u32,
    pub height: u32,
}

#[derive(Deserialize)]
struct ColorModes {
    light: ModeColors,
    dark: ModeColors,
}

#[derive(Deserialize)]
#[allow(non_snake_case)]
struct ModeColors {
    bg: String,
    surface: String,
    surfaceHover: String,
    text: String,
    textMuted: String,
    accent: AccentPair,
    islandBorder: String,
    controlBorder: String,
    controlFocusBorder: String,
    danger: String,
    desk: String,
    deskGrid: String,
    paper: String,
    paperBorder: String,
}

#[derive(Deserialize)]
struct AccentPair {
    teal: String,
    orange: String,
}

#[derive(Deserialize)]
struct PresetEntry {
    label: String,
    width: u32,
    height: u32,
}

fn read_tokens(root: &Path) -> Result<TokensFile> {
    let path = root.join("design").join("tokens.json");
    let text = fs::read_to_string(&path)
        .with_context(|| format!("reading design tokens at {}", path.display()))?;
    serde_json::from_str(&text).context("parsing tokens.json")
}

impl Theme {
    pub fn load(root: &Path, dark: bool) -> Result<Self> {
        let tokens = read_tokens(root)?;
        let mode = if dark {
            &tokens.color.dark
        } else {
            &tokens.color.light
        };
        Ok(Self {
            bg: parse_color(&mode.bg)?,
            surface: parse_color(&mode.surface)?,
            surface_hover: parse_color(&mode.surfaceHover)?,
            text: parse_color(&mode.text)?,
            text_muted: parse_color(&mode.textMuted)?,
            accent_teal: parse_color(&mode.accent.teal)?,
            accent_orange: parse_color(&mode.accent.orange)?,
            island_border: parse_color(&mode.islandBorder)?,
            control_border: parse_color(&mode.controlBorder)?,
            control_focus_border: parse_color(&mode.controlFocusBorder)?,
            danger: parse_color(&mode.danger)?,
            desk: parse_color(&mode.desk)?,
            desk_grid: parse_color(&mode.deskGrid)?,
            paper: parse_color(&mode.paper)?,
            paper_border: parse_color(&mode.paperBorder)?,
            metrics: Metrics {
                radius_sm: tokens.radius.sm,
                radius_md: tokens.radius.md,
                radius_lg: tokens.radius.lg,
                radius_window: tokens.radius.window,
                radius_island: tokens.radius.island,
                space_xs: tokens.space.xs,
                space_sm: tokens.space.sm,
                space_md: tokens.space.md,
                space_lg: tokens.space.lg,
                space_xl: tokens.space.xl,
                space_xxl: tokens.space.xxl,
                control_height: tokens.control.height,
                label_size: tokens.type_scale.labelSize,
                label_tracking: tokens.type_scale.labelTracking,
                body_size: tokens.type_scale.bodySize,
                title_size: tokens.type_scale.titleSize,
                brand_size: tokens.type_scale.brandSize,
                main_min_width: tokens.window.mainMinWidth as f32,
                main_min_height: tokens.window.mainMinHeight as f32,
                new_project_width: tokens.window.newProjectWidth,
                new_project_height: tokens.window.newProjectHeight,
                paste_min_width: tokens.window.pasteMinWidth,
                paste_max_width: tokens.window.pasteMaxWidth,
                paste_min_height: tokens.window.pasteMinHeight,
                paste_width_ratio: tokens.window.pasteWidthRatio,
            },
            presets: tokens
                .presets
                .into_iter()
                .map(|p| ResolutionPreset {
                    label: p.label,
                    width: p.width,
                    height: p.height,
                })
                .collect(),
        })
    }

    pub fn board_colors(&self) -> BoardColors {
        BoardColors {
            desk: color_rgba(self.desk),
            grid: color_rgba(self.desk_grid),
            paper_border: color_rgba(self.paper_border),
        }
    }

    pub fn window_metrics(root: &Path) -> Result<WindowMetrics> {
        let tokens = read_tokens(root)?;
        Ok(WindowMetrics {
            width: tokens.window.mainWidth,
            height: tokens.window.mainHeight,
        })
    }
}

fn parse_color(hex: &str) -> Result<Color> {
    let trimmed = hex.trim_start_matches('#');
    let len = trimmed.len().min(8);
    let value = u32::from_str_radix(&trimmed[..len.min(6).max(2)], 16)
        .with_context(|| format!("parsing color {hex}"))?;
    let r = ((value >> 16) & 0xFF) as u8;
    let g = ((value >> 8) & 0xFF) as u8;
    let b = (value & 0xFF) as u8;
    let a = if len >= 8 {
        u8::from_str_radix(&trimmed[6..8], 16).unwrap_or(255)
    } else {
        255
    };
    Ok(Color::from_argb_u8(a, r, g, b))
}

fn color_rgba(color: Color) -> [u8; 4] {
    let c = color.to_argb_u8();
    [c.red, c.green, c.blue, c.alpha]
}
