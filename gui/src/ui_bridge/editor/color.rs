use crate::shell::{hue_color, slint_color, AppController};
use crate::ui_bridge::{brush, AppWindow, ColorChrome};
use calumma_core::Tool;
use slint::{ComponentHandle, SharedString};

pub(super) fn sync_color_tips(ui: &AppWindow, controller: &AppController, tool: Tool) {
    let put = |key: &str| SharedString::from(controller.l10n.get(key));
    let chrome = ui.global::<ColorChrome>();
    if tool.takes_fill() {
        chrome.set_primary_tip(put("strokeColor"));
        chrome.set_secondary_tip(put("fillColor"));
    } else {
        chrome.set_primary_tip(put("primaryColor"));
        chrome.set_secondary_tip(put("secondaryColor"));
    }
    if tool == Tool::SelectColor {
        chrome.set_tertiary_tip(put("matchColor"));
    } else {
        chrome.set_tertiary_tip(put("tertiaryColor"));
    }
}

pub fn sync_color_picker(ui: &AppWindow, controller: &AppController) {
    let colors = &controller.quick_colors;
    let chrome = ui.global::<ColorChrome>();
    chrome.set_swatch0(brush(slint_color(colors.slots[0])));
    chrome.set_swatch1(brush(slint_color(colors.slots[1])));
    chrome.set_swatch2(brush(slint_color(colors.slots[2])));
    chrome.set_swatch3(brush(slint_color(colors.slots[3])));
    chrome.set_active_swatch(colors.active as i32);
    chrome.set_hue_color(hue_color(colors.hsb.hue));
    chrome.set_sb_x(colors.hsb.saturation);
    chrome.set_sb_y(1.0 - colors.hsb.brightness);
    chrome.set_hue_x(colors.hsb.hue);
    chrome.set_hex_text(SharedString::from(colors.hex_text().as_str()));
}
