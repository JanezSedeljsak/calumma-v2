import Foundation

enum CalmTextAlign: UInt32, CaseIterable, Identifiable {
    case left = 0
    case center = 1
    case right = 2

    var id: UInt32 { rawValue }
}

/// Caret motions the engine understands. The shell maps AppKit selectors onto these and
/// sends them straight through — where the caret lands in a wrapped, shaped, bidirectional
/// run is layout, and layout is the engine's.
enum CalmCaretStep: UInt32 {
    case left = 0
    case right = 1
    case up = 2
    case down = 3
    case lineStart = 4
    case lineEnd = 5
    case docStart = 6
    case docEnd = 7
}

/// One row of the font picker, as the engine reports it: the family name plus the cuts the
/// system really ships, so B and I can be offered only where they exist.
struct CalmFontFamily: Identifiable, Hashable {
    let name: String
    let hasBold: Bool
    let hasItalic: Bool

    var id: String { name }
}

extension Engine {
    /// Every installed font family, read once from the engine. The shell must not ask
    /// AppKit for this — the engine owns the font database that actually renders the glyphs,
    /// and asking anything else risks offering a family it cannot draw.
    static let fontFamilies: [CalmFontFamily] = {
        (0..<calm_font_family_count()).compactMap { index in
            guard let namePtr = calm_font_family_name(index) else { return nil }
            let name = String(cString: namePtr)
            calm_string_free(namePtr)
            let styles = calm_font_family_styles(index)
            return CalmFontFamily(
                name: name,
                hasBold: styles & UInt32(CalmFontStyleBold.rawValue) != 0,
                hasItalic: styles & UInt32(CalmFontStyleItalic.rawValue) != 0
            )
        }
    }()

    static var textSizeRange: ClosedRange<Float> {
        calm_text_size_min()...calm_text_size_max()
    }

    static var textLineHeightRange: ClosedRange<Float> {
        calm_text_line_height_min()...calm_text_line_height_max()
    }

    /// The cuts the active family ships, for greying out the style buttons.
    var activeFontFamily: CalmFontFamily? {
        Engine.fontFamilies.first { $0.name == textFamily }
    }

    func syncTextState() {
        guard let ptr else { return }
        let editing = calm_engine_text_editing(ptr) == 1
        if textEditing != editing {
            textEditing = editing
        }
        if let familyPtr = calm_engine_text_family(ptr) {
            let family = String(cString: familyPtr)
            calm_string_free(familyPtr)
            if textFamily != family {
                textFamily = family
            }
        }
        let size = calm_engine_text_size(ptr)
        if textSize != size {
            textSize = size
        }
        let align = CalmTextAlign(rawValue: calm_engine_text_align(ptr)) ?? .left
        if textAlign != align {
            textAlign = align
        }
        let lineHeight = calm_engine_text_line_height(ptr)
        if textLineHeight != lineHeight {
            textLineHeight = lineHeight
        }
        let styles = calm_engine_text_styles(ptr)
        let bold = styles & UInt32(CalmFontStyleBold.rawValue) != 0
        if textBold != bold {
            textBold = bold
        }
        let italic = styles & UInt32(CalmFontStyleItalic.rawValue) != 0
        if textItalic != italic {
            textItalic = italic
        }
    }

    /// Typing. `render()` runs on the display link, so the only thing needed here is to let
    /// the engine repaint — the glyphs are on screen on the next frame.
    func textInsert(_ text: String) {
        guard let ptr, !text.isEmpty else { return }
        _ = text.withCString { calm_engine_text_insert(ptr, $0) }
        syncTextState()
    }

    func textSetMarked(_ text: String) {
        guard let ptr else { return }
        _ = text.withCString { calm_engine_text_set_marked(ptr, $0) }
    }

    func textBackspace() {
        guard let ptr else { return }
        _ = calm_engine_text_backspace(ptr)
    }

    func textDeleteForward() {
        guard let ptr else { return }
        _ = calm_engine_text_delete_forward(ptr)
    }

    func textMoveCaret(_ step: CalmCaretStep) {
        guard let ptr else { return }
        _ = calm_engine_text_move_caret(ptr, step.rawValue)
    }

    func commitText() {
        guard let ptr, calm_engine_text_editing(ptr) == 1 else { return }
        _ = calm_engine_text_commit(ptr)
        syncState()
        refreshLayers()
    }

    func editTextLayer(_ index: Int) {
        guard let ptr else { return }
        _ = calm_engine_text_edit_layer(ptr, UInt32(index))
        syncState()
        refreshLayers()
    }

    func isLayerText(index: Int) -> Bool {
        layerIsText.indices.contains(index) ? layerIsText[index] : false
    }

    func setTextFamily(_ family: String) {
        guard let ptr else { return }
        _ = family.withCString { calm_engine_set_text_family(ptr, $0) }
        syncTextState()
    }

    func setTextSize(_ size: Float) {
        guard let ptr else { return }
        _ = calm_engine_set_text_size(ptr, size)
        syncTextState()
    }

    func setTextAlign(_ align: CalmTextAlign) {
        guard let ptr else { return }
        _ = calm_engine_set_text_align(ptr, align.rawValue)
        syncTextState()
    }

    func setTextBold(_ bold: Bool) {
        guard let ptr else { return }
        _ = calm_engine_set_text_bold(ptr, bold ? 1 : 0)
        syncTextState()
    }

    func setTextItalic(_ italic: Bool) {
        guard let ptr else { return }
        _ = calm_engine_set_text_italic(ptr, italic ? 1 : 0)
        syncTextState()
    }

    func setTextLineHeight(_ lineHeight: Float) {
        guard let ptr else { return }
        _ = calm_engine_set_text_line_height(ptr, lineHeight)
        syncTextState()
    }

    /// Turns a text layer into ordinary pixels so paint tools can reach it. One way — the
    /// run is gone afterwards, which is why it is an explicit action and not a side effect
    /// of picking up a brush.
    func rasterizeTextLayer(_ index: Int) {
        guard let ptr else { return }
        _ = calm_engine_rasterize_text_layer(ptr, UInt32(index))
        syncState()
        refreshLayers()
    }

    /// Caret position in board-view coordinates, for anchoring the IME candidate window.
    func textCaretRect() -> (x: Float, y: Float, height: Float)? {
        guard let ptr else { return nil }
        var x: Float = 0
        var y: Float = 0
        var height: Float = 0
        guard calm_engine_text_caret_rect(ptr, &x, &y, &height) == CalmStatusOk else {
            return nil
        }
        return (x, y, height)
    }

    func layerText(index: Int) -> String? {
        guard let ptr, let textPtr = calm_engine_layer_text(ptr, UInt32(index)) else {
            return nil
        }
        let text = String(cString: textPtr)
        calm_string_free(textPtr)
        return text
    }
}
