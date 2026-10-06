use super::{put, put_rows};
use crate::shell::AppController;
use crate::ui_bridge::{AppWindow, LayerChrome, LayerListChrome, LayerRow};
use slint::{ComponentHandle, SharedString};

fn signed_percent(value: f32) -> String {
    format!("{:+}", (value * 100.0).round() as i32)
}

pub fn apply_filter_readout(ui: &AppWindow, kind: i32, value: f32) {
    let chrome = ui.global::<LayerChrome>();
    match kind {
        0 => chrome.set_brightness_text(put(signed_percent(value))),
        1 => chrome.set_contrast_text(put(signed_percent(value))),
        2 => chrome.set_vibrance_text(put(signed_percent(value))),
        3 => chrome.set_saturation_text(put(signed_percent(value))),
        4 => chrome.set_gamma_text(put(format!("{value:.2}"))),
        5 => chrome.set_hue_text(put(format!("{:+}°", value.round() as i32))),
        _ => {}
    }
}

pub fn apply_opacity_readout(ui: &AppWindow, value: f32) {
    let chrome = ui.global::<LayerChrome>();
    chrome.set_opacity_text(put(format!("{}", (value * 100.0).round() as i32)));
}

pub fn sync_layer_settings(ui: &AppWindow, controller: &AppController) {
    if controller.layer_settings_dragging {
        // A slider is mid-drag: its committed value lags the live one on
        // screen, so an engine-sourced resync here would yank the slider
        // back to the stale value until the drag settles.
        return;
    }
    let chrome = ui.global::<LayerChrome>();
    if let Some(layer) = controller.layer_settings_summary() {
        let engine = controller.engine.borrow();
        let index = layer.index;
        let opacity = engine.layer_opacity(index);
        let adjustments = engine.layer_adjustments(index);
        let clipped = engine.is_layer_clipped(index);
        let masked = engine.is_layer_masked(index);
        chrome.set_name(SharedString::from(layer.name.as_str()));
        chrome.set_visible(layer.visible);
        chrome.set_locked(layer.locked);
        chrome.set_can_delete(!layer.is_paper);
        chrome.set_can_rename(engine.can_rename_layer(index));
        chrome.set_opacity(opacity);
        chrome.set_opacity_text(put(format!("{}", (opacity * 100.0).round() as i32)));
        chrome.set_preview(controller.thumb_cache.preview_image(index));
        let blend = engine.layer_blend_mode(index);
        chrome.set_blend(blend.as_u32() as i32);
        chrome.set_blend_current_label(put(controller
            .l10n
            .get(crate::shell::blend_label_key(blend))));
        chrome.set_brightness(adjustments.brightness);
        chrome.set_contrast(adjustments.contrast);
        chrome.set_vibrance(adjustments.vibrance);
        chrome.set_saturation(adjustments.saturation);
        chrome.set_gamma(adjustments.levels_gamma);
        chrome.set_hue(adjustments.hue);
        chrome.set_brightness_text(put(signed_percent(adjustments.brightness)));
        chrome.set_contrast_text(put(signed_percent(adjustments.contrast)));
        chrome.set_vibrance_text(put(signed_percent(adjustments.vibrance)));
        chrome.set_saturation_text(put(signed_percent(adjustments.saturation)));
        chrome.set_gamma_text(put(format!("{:.2}", adjustments.levels_gamma)));
        chrome.set_hue_text(put(format!("{:+}°", adjustments.hue.round() as i32)));
        chrome.set_clipped(clipped);
        chrome.set_masked(masked);
        chrome.set_can_clip(if clipped {
            engine.can_release_clipping_mask(index)
        } else {
            engine.can_create_clipping_mask(index)
        });
        chrome.set_can_mask(if masked {
            engine.can_release_layer_mask(index)
        } else {
            engine.can_create_layer_mask(index)
        });
        chrome.set_can_flatten(engine.can_flatten_clip(index));
        chrome.set_can_merge(engine.can_merge_layer_down(index));
        chrome.set_can_reset_transform(engine.layer_has_transform(index) && !layer.locked);
        chrome.set_can_move_up(engine.can_move_layer_up(index));
        chrome.set_can_move_down(engine.can_move_layer_down(index));
        chrome.set_can_rasterize(engine.layer_is_rasterizable(index));
        chrome.set_lock_label(put(controller.l10n.get(if layer.locked {
            "layerUnlock"
        } else {
            "layerLock"
        })));
        chrome.set_clip_label(put(controller.l10n.get(if clipped {
            "releaseClippingMask"
        } else {
            "createClippingMask"
        })));
        chrome.set_mask_label(put(controller.l10n.get(if masked {
            "releaseLayerMask"
        } else {
            "addLayerMask"
        })));
        chrome.set_flatten_label(put(controller.l10n.get(if masked {
            "applyLayerMask"
        } else {
            "flattenClippingMask"
        })));
        drop(engine);
    }
    if let Some(index) = controller.layer_hover_index {
        let engine = controller.engine.borrow();
        let name = engine
            .list_layers()
            .into_iter()
            .find(|layer| layer.index == index)
            .map(|layer| layer.name)
            .unwrap_or_default();
        let list = ui.global::<LayerListChrome>();
        list.set_hover_visible(true);
        list.set_hover_name(SharedString::from(name.as_str()));
        list.set_hover_image(controller.thumb_cache.preview_image(index));
    } else {
        ui.global::<LayerListChrome>().set_hover_visible(false);
    }
}

fn sync_layer_bounds(ui: &AppWindow, controller: &AppController) {
    let list = ui.global::<LayerListChrome>();
    let engine = controller.engine.borrow();
    let index = engine.active_layer_index();
    let bounds = index.and_then(|index| engine.layer_bounds(index));
    if let Some((x, y, x1, y1)) = bounds {
        list.set_layer_x_text(put(format!("{}", x.round() as i32)));
        list.set_layer_y_text(put(format!("{}", y.round() as i32)));
        list.set_layer_w_text(put(format!("{}", (x1 - x).round() as i32)));
        list.set_layer_h_text(put(format!("{}", (y1 - y).round() as i32)));
    } else {
        list.set_layer_x_text(put("0".into()));
        list.set_layer_y_text(put("0".into()));
        list.set_layer_w_text(put("0".into()));
        list.set_layer_h_text(put("0".into()));
    }
}

pub fn sync_layer_rows(ui: &AppWindow, controller: &mut AppController) {
    let thumbs_changed = controller
        .thumb_cache
        .sync(&controller.engine.borrow(), controller.prefs.is_dark());
    if thumbs_changed {
        let engine = controller.engine.borrow();
        let selection = engine.layer_selection();
        let rows: Vec<LayerRow> = engine
            .list_layers()
            .iter()
            .map(|layer| LayerRow {
                index: layer.index as i32,
                name: SharedString::from(layer.name.as_str()),
                visible: layer.visible,
                locked: layer.locked,
                active: layer.active,
                selected: selection.contains(&layer.index),
                paper: layer.is_paper,
                clipped: layer.clipped,
                masked: layer.masked,
                clip_base: layer.clip_base,
                thumb: controller.thumb_cache.row_image(layer.index),
            })
            .collect();
        drop(engine);
        let list = ui.global::<LayerListChrome>();
        list.set_layers(put_rows(list.get_layers(), rows));
        ui.global::<LayerChrome>()
            .set_align_visible(selection.len() >= 2);
    }
    sync_layer_settings(ui, controller);
}

pub fn sync_layers(ui: &AppWindow, controller: &mut AppController) {
    sync_layer_rows(ui, controller);
    sync_layer_bounds(ui, controller);
    super::tools::sync_tool_gate(ui, controller);
}
