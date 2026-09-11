use calumma_core::blend::{blend_rgb, lum, sat};
use calumma_core::layer::BlendMode;
use calumma_core::tile::blend_with_mode;

fn channel(mode: BlendMode, b: f32, s: f32) -> f32 {
    blend_rgb(mode, [b; 3], [s; 3])[0]
}

#[test]
fn every_mode_is_in_the_menu_exactly_once() {
    let listed: Vec<BlendMode> = BlendMode::MENU
        .iter()
        .flat_map(|g| g.iter().copied())
        .collect();
    let mut value = 0;
    while let Some(mode) = BlendMode::from_u32(value) {
        assert_eq!(listed.iter().filter(|&&m| m == mode).count(), 1, "{mode:?}");
        value += 1;
    }
    assert_eq!(listed.len(), value as usize);
}

#[test]
fn the_original_three_keep_the_numbers_projects_stored() {
    assert_eq!(BlendMode::Normal.as_u32(), 0);
    assert_eq!(BlendMode::Multiply.as_u32(), 1);
    assert_eq!(BlendMode::Screen.as_u32(), 2);
    assert!(BlendMode::Screen.is_fixed_function());
    assert!(!BlendMode::Overlay.is_fixed_function());
}

#[test]
fn separable_modes_match_their_definitions() {
    assert_eq!(channel(BlendMode::Darken, 0.8, 0.3), 0.3);
    assert_eq!(channel(BlendMode::Lighten, 0.8, 0.3), 0.8);
    assert!((channel(BlendMode::Difference, 0.8, 0.3) - 0.5).abs() < 1e-6);
    assert_eq!(channel(BlendMode::LinearDodge, 0.8, 0.5), 1.0);
    assert!((channel(BlendMode::LinearBurn, 0.8, 0.5) - 0.3).abs() < 1e-6);
    assert!((channel(BlendMode::Subtract, 0.8, 0.3) - 0.5).abs() < 1e-6);
    assert_eq!(channel(BlendMode::Divide, 0.5, 0.0), 1.0);
    assert!((channel(BlendMode::Overlay, 0.25, 0.5) - 0.25).abs() < 1e-6);
    assert!((channel(BlendMode::HardLight, 0.5, 0.25) - 0.25).abs() < 1e-6);
    assert!((channel(BlendMode::Exclusion, 0.5, 0.5) - 0.5).abs() < 1e-6);
    assert_eq!(channel(BlendMode::HardMix, 0.6, 0.5), 1.0);
    assert_eq!(channel(BlendMode::HardMix, 0.4, 0.5), 0.0);
    assert_eq!(channel(BlendMode::ColorDodge, 0.5, 1.0), 1.0);
    assert_eq!(channel(BlendMode::ColorBurn, 0.5, 0.0), 0.0);
}

#[test]
fn a_mid_grey_source_leaves_the_neutral_light_modes_alone() {
    for mode in [
        BlendMode::SoftLight,
        BlendMode::HardLight,
        BlendMode::VividLight,
        BlendMode::LinearLight,
        BlendMode::PinLight,
    ] {
        assert!(
            (channel(mode, 0.3, 0.5) - 0.3).abs() < 1e-4,
            "{mode:?} should leave the backdrop at 50% grey"
        );
    }
}

#[test]
fn component_modes_trade_hue_and_luminosity() {
    let backdrop = [0.2, 0.4, 0.8];
    let source = [0.9, 0.3, 0.1];
    let luminosity = blend_rgb(BlendMode::Luminosity, backdrop, source);
    assert!((lum(luminosity) - lum(source)).abs() < 1e-4);
    let color = blend_rgb(BlendMode::Color, backdrop, source);
    assert!((lum(color) - lum(backdrop)).abs() < 1e-4);
    let saturation = blend_rgb(BlendMode::Saturation, backdrop, source);
    assert!((sat(saturation) - sat(source)).abs() < 1e-4);
}

#[test]
fn darker_and_lighter_color_pick_a_whole_pixel() {
    let dark = [0.1, 0.2, 0.3];
    let light = [0.9, 0.1, 0.9];
    assert_eq!(blend_rgb(BlendMode::DarkerColor, light, dark), dark);
    assert_eq!(blend_rgb(BlendMode::LighterColor, dark, light), light);
}

#[test]
fn a_blend_over_nothing_is_the_source_itself() {
    let src = [200, 100, 50, 255];
    assert_eq!(
        blend_with_mode([0, 0, 0, 0], src, BlendMode::Difference),
        src
    );
}

#[test]
fn difference_against_an_opaque_backdrop() {
    assert_eq!(
        blend_with_mode(
            [255, 255, 255, 255],
            [55, 155, 255, 255],
            BlendMode::Difference
        ),
        [200, 100, 0, 255]
    );
}
