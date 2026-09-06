#include "Engine.hpp"

// Text editing and text style. The shell owns no caret, no selection range and no measurement:
// an input-method event becomes an insert or a marked string, arrow keys become caret steps,
// and where the caret sits on screen is asked for rather than worked out.

namespace calumma {

bool Engine::textInsert(const std::string &text) {
    return call([&](CalmEngine *e) { return calm_engine_text_insert(e, text.c_str()); });
}

bool Engine::textSetMarked(const std::string &text) {
    return call([&](CalmEngine *e) { return calm_engine_text_set_marked(e, text.c_str()); });
}

bool Engine::textBackspace() {
    return call([](CalmEngine *e) { return calm_engine_text_backspace(e); });
}

bool Engine::textDeleteForward() {
    return call([](CalmEngine *e) { return calm_engine_text_delete_forward(e); });
}

bool Engine::textMoveCaret(uint32_t step, bool extend) {
    return call([&](CalmEngine *e) { return calm_engine_text_move_caret(e, step, extend ? 1 : 0); });
}

bool Engine::textSelectAll() {
    return call([](CalmEngine *e) { return calm_engine_text_select_all(e); });
}

bool Engine::textSelectWordAt(float x, float y) {
    return call([&](CalmEngine *e) { return calm_engine_text_select_word_at(e, x, y); });
}

bool Engine::textSelectParagraphAt(float x, float y) {
    return call([&](CalmEngine *e) { return calm_engine_text_select_paragraph_at(e, x, y); });
}

bool Engine::textHasSelection() const {
    return query([](CalmEngine *e) { return calm_engine_text_has_selection(e); }, 0) > 0;
}

bool Engine::textCommit() {
    return call([](CalmEngine *e) { return calm_engine_text_commit(e); });
}

bool Engine::textEditLayer(uint32_t index) {
    return call([&](CalmEngine *e) { return calm_engine_text_edit_layer(e, index); });
}

bool Engine::textEditing() const {
    return query([](CalmEngine *e) { return calm_engine_text_editing(e); }, 0) > 0;
}

bool Engine::textCaretRect(CaretRect &out) const {
    return call([&](CalmEngine *e) {
        return calm_engine_text_caret_rect(e, &out.x, &out.y, &out.height);
    });
}

std::string Engine::layerText(uint32_t index) const {
    if (!m_engine) {
        return {};
    }
    return ownedString(calm_engine_layer_text(m_engine.get(), index));
}

bool Engine::setTextFamily(const std::string &family) {
    return call([&](CalmEngine *e) { return calm_engine_set_text_family(e, family.c_str()); });
}

bool Engine::setTextSize(float size) {
    return call([&](CalmEngine *e) { return calm_engine_set_text_size(e, size); });
}

bool Engine::setTextAlign(uint32_t align) {
    return call([&](CalmEngine *e) { return calm_engine_set_text_align(e, align); });
}

bool Engine::setTextBold(bool bold) {
    return call([&](CalmEngine *e) { return calm_engine_set_text_bold(e, bold ? 1 : 0); });
}

bool Engine::setTextItalic(bool italic) {
    return call([&](CalmEngine *e) { return calm_engine_set_text_italic(e, italic ? 1 : 0); });
}

bool Engine::setTextLineHeight(float lineHeight) {
    return call([&](CalmEngine *e) { return calm_engine_set_text_line_height(e, lineHeight); });
}

bool Engine::setTextWrapWidth(float width) {
    return call([&](CalmEngine *e) { return calm_engine_set_text_wrap_width(e, width); });
}

std::string Engine::textFamily() const {
    if (!m_engine) {
        return {};
    }
    return ownedString(calm_engine_text_family(m_engine.get()));
}

float Engine::textSize() const {
    return query([](CalmEngine *e) { return calm_engine_text_size(e); }, 0.0f);
}

uint32_t Engine::textAlign() const {
    return query([](CalmEngine *e) { return calm_engine_text_align(e); }, 0u);
}

float Engine::textLineHeight() const {
    return query([](CalmEngine *e) { return calm_engine_text_line_height(e); }, 0.0f);
}

uint32_t Engine::textStyles() const {
    return query([](CalmEngine *e) { return calm_engine_text_styles(e); }, 0u);
}

float Engine::textWrapWidth() const {
    return query([](CalmEngine *e) { return calm_engine_text_wrap_width(e); }, 0.0f);
}

float Engine::textWrapMax() const {
    return query([](CalmEngine *e) { return calm_engine_text_wrap_max(e); }, 0.0f);
}

}  // namespace calumma
