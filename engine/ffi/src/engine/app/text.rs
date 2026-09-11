use super::Engine;
use calumma_core::{
    font_families as list_font_families, font_family_styles, text_size_from_unit, text_size_unit,
    Document, Step, TextAlign, Tool, TEXT_LINE_HEIGHT_MAX, TEXT_LINE_HEIGHT_MIN,
    TEXT_WRAP_MIN_WIDTH,
};

#[derive(Clone, Debug)]
pub struct FontFamilyInfo {
    pub name: String,
    pub has_bold: bool,
    pub has_italic: bool,
}

impl Engine {
    pub fn font_families() -> Vec<FontFamilyInfo> {
        list_font_families()
            .into_iter()
            .map(|name| {
                let (has_bold, has_italic) = font_family_styles(&name);
                FontFamilyInfo {
                    name,
                    has_bold,
                    has_italic,
                }
            })
            .collect()
    }

    pub fn text_family(&self) -> String {
        self.with_doc(|doc| doc.active_text_style().family)
            .unwrap_or_default()
    }

    pub fn text_size(&self) -> f32 {
        self.with_doc(|doc| doc.active_text_style().size)
            .unwrap_or(0.0)
    }

    pub fn text_size_unit(&self) -> f32 {
        text_size_unit(self.text_size())
    }

    pub fn set_text_size_unit(&mut self, unit: f32) {
        self.set_text_size(text_size_from_unit(unit));
    }

    pub fn text_bold(&self) -> bool {
        self.with_doc(|doc| doc.active_text_style().bold)
            .unwrap_or(false)
    }

    pub fn text_italic(&self) -> bool {
        self.with_doc(|doc| doc.active_text_style().italic)
            .unwrap_or(false)
    }

    pub fn text_can_bold(&self) -> bool {
        font_family_styles(&self.text_family()).0
    }

    pub fn text_can_italic(&self) -> bool {
        font_family_styles(&self.text_family()).1
    }

    pub fn text_align(&self) -> TextAlign {
        self.with_doc(|doc| {
            doc.active_text_run()
                .map(|run| run.align)
                .unwrap_or(doc.text_style.align)
        })
        .unwrap_or(TextAlign::Left)
    }

    pub fn text_line_height(&self) -> f32 {
        self.with_doc(|doc| {
            doc.active_text_run()
                .map(|run| run.line_height)
                .unwrap_or(doc.text_style.line_height)
        })
        .unwrap_or(1.0)
    }

    pub fn text_wrap_width(&self) -> f32 {
        self.with_doc(|doc| doc.text_wrap_width().unwrap_or(0.0))
            .unwrap_or(0.0)
    }

    pub fn text_wrap_max(&self) -> f32 {
        self.with_doc(|doc| doc.width as f32).unwrap_or(0.0)
    }

    pub fn set_text_family(&mut self, family: &str) -> bool {
        self.mutate_text(|doc| doc.set_text_family(family))
            .unwrap_or(false)
    }

    pub fn set_text_size(&mut self, size: f32) {
        self.mutate_text(|doc| {
            doc.set_text_size(size);
            true
        });
    }

    pub fn set_text_bold(&mut self, bold: bool) {
        self.mutate_text(|doc| {
            doc.set_text_bold(bold);
            true
        });
    }

    pub fn set_text_italic(&mut self, italic: bool) {
        self.mutate_text(|doc| {
            doc.set_text_italic(italic);
            true
        });
    }

    pub fn set_text_align(&mut self, align: TextAlign) {
        self.mutate_text(|doc| {
            doc.set_text_align(align);
            true
        });
    }

    pub fn set_text_line_height(&mut self, line_height: f32) {
        let line_height = line_height.clamp(TEXT_LINE_HEIGHT_MIN, TEXT_LINE_HEIGHT_MAX);
        self.mutate_text(|doc| {
            doc.set_text_line_height(line_height);
            true
        });
    }

    pub fn set_text_wrap_width(&mut self, width: f32) {
        let wrap = if width >= TEXT_WRAP_MIN_WIDTH {
            Some(width)
        } else {
            None
        };
        self.mutate_text(|doc| {
            doc.set_text_wrap_width(wrap);
            true
        });
    }

    pub fn text_insert(&mut self, text: &str) {
        self.edit_text_session(|doc| doc.text_insert(text));
    }

    pub fn text_backspace(&mut self) {
        self.edit_text_session(Document::text_backspace);
    }

    pub fn text_delete_forward(&mut self) {
        self.edit_text_session(Document::text_delete_forward);
    }

    pub fn text_delete_word(&mut self, forward: bool) {
        self.edit_text_session(|doc| doc.text_delete_word(forward));
    }

    pub fn text_step_caret(&mut self, step: Step, extend: bool) {
        self.edit_text_session(|doc| doc.text_step_caret(step, extend));
    }

    pub fn text_select_all(&mut self) -> bool {
        self.edit_text_session(Document::text_select_all)
            .unwrap_or(false)
    }

    pub fn commit_text(&mut self) {
        let mut inner = self.inner.lock();
        let Some(doc) = inner.doc.as_mut() else {
            return;
        };
        if !doc.text_editing() {
            return;
        }
        doc.commit_text();
        inner.dirty_save = true;
        inner.invalidate_renderer();
    }

    fn edit_text_session<R>(&mut self, f: impl FnOnce(&mut Document) -> R) -> Option<R> {
        let mut inner = self.inner.lock();
        let doc = inner.doc.as_mut().filter(|doc| doc.text_editing())?;
        let out = f(doc);
        inner.dirty_save = true;
        inner.invalidate_renderer();
        Some(out)
    }

    fn with_doc<R>(&self, f: impl FnOnce(&Document) -> R) -> Option<R> {
        self.inner.lock().doc.as_ref().map(f)
    }

    fn mutate_text<R>(&mut self, f: impl FnOnce(&mut Document) -> R) -> Option<R> {
        let mut inner = self.inner.lock();
        let doc = inner.doc.as_mut()?;
        ensure_text_session(doc);
        let out = f(doc);
        inner.dirty_save = true;
        inner.invalidate_renderer();
        Some(out)
    }
}

fn ensure_text_session(doc: &mut Document) {
    if doc.text_editing() || doc.tool != Tool::Text {
        return;
    }
    let index = doc.active_layer;
    if doc.layers.get(index).is_some_and(|layer| layer.is_text()) {
        doc.edit_text_layer(index);
    }
}
