use super::schedule_toast_hide;
use crate::shell::SharedController;
use crate::ui_bridge::{sync_shell, AppWindow, MenuChrome, SharedUi};
use calumma_io::RasterFormat;
use slint::ComponentHandle;

pub fn wire(ui: &AppWindow, controller: SharedController, ui_weak: SharedUi) {
    let export_raster = |format: RasterFormat, ext: &'static str| {
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let mut ctrl = controller.borrow_mut();
            let result = ctrl.try_save_composite(format, ext);
            ctrl.notify_export(result);
            if ctrl.toast_visible {
                if let Some(ui) = ui_weak.upgrade() {
                    sync_shell(&ui, &ctrl);
                }
                schedule_toast_hide(&ui_weak, controller.clone());
            }
        }
    };

    ui.global::<MenuChrome>()
        .on_export_png(export_raster(RasterFormat::Png, "png"));
    ui.global::<MenuChrome>()
        .on_export_jpeg(export_raster(RasterFormat::Jpeg, "jpg"));
    ui.global::<MenuChrome>()
        .on_export_webp(export_raster(RasterFormat::Webp, "webp"));
    ui.global::<MenuChrome>()
        .on_export_avif(export_raster(RasterFormat::Avif, "avif"));
    ui.global::<MenuChrome>()
        .on_export_heic(export_raster(RasterFormat::Heic, "heic"));

    ui.global::<MenuChrome>().on_export_psd({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let mut ctrl = controller.borrow_mut();
            let result = ctrl.try_save_psd();
            ctrl.notify_export(result);
            if ctrl.toast_visible {
                if let Some(ui) = ui_weak.upgrade() {
                    sync_shell(&ui, &ctrl);
                }
                schedule_toast_hide(&ui_weak, controller.clone());
            }
        }
    });
    ui.global::<MenuChrome>().on_export_svg({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let mut ctrl = controller.borrow_mut();
            let result = ctrl.try_save_svg();
            ctrl.notify_export(result);
            if ctrl.toast_visible {
                if let Some(ui) = ui_weak.upgrade() {
                    sync_shell(&ui, &ctrl);
                }
                schedule_toast_hide(&ui_weak, controller.clone());
            }
        }
    });
    ui.global::<MenuChrome>().on_export_pdf({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let mut ctrl = controller.borrow_mut();
            let result = ctrl.try_save_pdf();
            ctrl.notify_export(result);
            if ctrl.toast_visible {
                if let Some(ui) = ui_weak.upgrade() {
                    sync_shell(&ui, &ctrl);
                }
                schedule_toast_hide(&ui_weak, controller.clone());
            }
        }
    });
}
