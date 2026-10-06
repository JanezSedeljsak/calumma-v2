use super::{put, put_rows};
use crate::shell::AppController;
use crate::ui_bridge::{FontFamilyRow, ToolChrome};
use calumma_app::Engine;
use calumma_core::{Tool, TEXT_LINE_HEIGHT_MAX, TEXT_LINE_HEIGHT_MIN};
use slint::SharedString;

fn font_rows(query: &str) -> Vec<FontFamilyRow> {
    let query = query.trim().to_lowercase();
    Engine::font_families()
        .into_iter()
        .filter(|family| query.is_empty() || family.name.to_lowercase().contains(&query))
        .map(|family| FontFamilyRow {
            name: SharedString::from(family.name),
            has_bold: family.has_bold,
            has_italic: family.has_italic,
        })
        .collect()
}

pub(super) fn sync_text_chrome(chrome: ToolChrome, controller: &AppController, tool: Tool) {
    chrome.set_text_line_height_min(TEXT_LINE_HEIGHT_MIN);
    chrome.set_text_line_height_max(TEXT_LINE_HEIGHT_MAX);
    if tool != Tool::Text {
        return;
    }
    let engine = controller.engine.borrow();
    let size = engine.text_size();
    let line_height = engine.text_line_height();
    let wrap = engine.text_wrap_width();
    let wrap_max = engine.text_wrap_max();
    chrome.set_text_family(put(engine.text_family()));
    chrome.set_text_size_unit(engine.text_size_unit());
    chrome.set_text_size_text(put(format!("{}", size.round() as i32)));
    chrome.set_text_line_height(line_height);
    chrome.set_text_line_height_text(put(format!("{line_height:.1}")));
    chrome.set_text_wrap_width(wrap);
    chrome.set_text_wrap_max(wrap_max);
    chrome.set_text_wrap_text(put(format!("{}", wrap.round() as i32)));
    chrome.set_text_bold(engine.text_bold());
    chrome.set_text_italic(engine.text_italic());
    chrome.set_text_can_bold(engine.text_can_bold());
    chrome.set_text_can_italic(engine.text_can_italic());
    chrome.set_text_align(engine.text_align() as i32);
    drop(engine);
    let query = chrome.get_text_font_query();
    chrome.set_font_families(put_rows(
        chrome.get_font_families(),
        font_rows(query.as_str()),
    ));
}
