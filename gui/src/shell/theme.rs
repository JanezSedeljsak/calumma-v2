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
    pub tool_selected: Color,
    pub danger: Color,
    pub desk: Color,
    pub desk_grid: Color,
    pub paper_border: Color,
    pub presets: Vec<ResolutionPreset>,
}

#[derive(Clone, Debug)]
pub struct ResolutionPreset {
    pub label: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Deserialize)]
struct TokensFile {
    presets: Vec<PresetEntry>,
    color: ColorModes,
    window: WindowTokens,
}

#[derive(Deserialize)]
#[allow(non_snake_case)]
struct WindowTokens {
    mainWidth: u32,
    mainHeight: u32,
    mainMinWidth: u32,
    mainMinHeight: u32,
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

impl Theme {
    pub fn load(root: &Path, dark: bool) -> Result<Self> {
        let path = root.join("design").join("tokens.json");
        let text = fs::read_to_string(&path)
            .with_context(|| format!("reading design tokens at {}", path.display()))?;
        let tokens: TokensFile = serde_json::from_str(&text).context("parsing tokens.json")?;
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
            tool_selected: parse_color(&mode.surfaceHover)?,
            danger: parse_color(&mode.danger)?,
            desk: parse_color(&mode.desk)?,
            desk_grid: parse_color(&mode.deskGrid)?,
            paper_border: parse_color(&mode.paperBorder)?,
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
        let path = root.join("design").join("tokens.json");
        let text = fs::read_to_string(&path)
            .with_context(|| format!("reading design tokens at {}", path.display()))?;
        let tokens: TokensFile = serde_json::from_str(&text).context("parsing tokens.json")?;
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
