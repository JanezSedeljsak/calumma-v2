import AppKit

/// Typing on the board.
///
/// Keystrokes go through `interpretKeyEvents`, so the board gets everything AppKit already
/// knows how to do — dead keys, accent popovers, emoji picker, CJK input methods, the
/// standard caret-motion selectors — instead of a hand-rolled character switch. Each of
/// those arrives here as either text to insert or a selector to translate, and both are
/// forwarded to the engine unchanged: the shell decides *that* a key happened, never what
/// it means for the layout.
extension BoardMTKView: NSTextInputClient {
    var textEngine: Engine? {
        boardCoordinator?.engine
    }

    var isTypingOnBoard: Bool {
        MainActor.assumeIsolated { app?.engine.textEditing ?? false }
    }

    func insertText(_ string: Any, replacementRange: NSRange) {
        let text: String
        switch string {
        case let value as String: text = value
        case let value as NSAttributedString: text = value.string
        default: return
        }
        markedTextValue = ""
        textEngine?.textInsert(text)
    }

    /// An in-flight composition. The engine renders it on the board at the caret, so a
    /// half-typed `¨` or a Japanese reading is visible in place rather than in a floating
    /// box over the artwork.
    func setMarkedText(_ string: Any, selectedRange: NSRange, replacementRange: NSRange) {
        let text: String
        switch string {
        case let value as String: text = value
        case let value as NSAttributedString: text = value.string
        default: text = ""
        }
        markedTextValue = text
        textEngine?.textSetMarked(text)
    }

    func unmarkText() {
        markedTextValue = ""
        textEngine?.textSetMarked("")
    }

    func hasMarkedText() -> Bool {
        !markedTextValue.isEmpty
    }

    func markedRange() -> NSRange {
        markedTextValue.isEmpty
            ? NSRange(location: NSNotFound, length: 0)
            : NSRange(location: 0, length: markedTextValue.utf16.count)
    }

    func selectedRange() -> NSRange {
        NSRange(location: 0, length: 0)
    }

    func attributedSubstring(
        forProposedRange range: NSRange,
        actualRange: NSRangePointer?
    ) -> NSAttributedString? {
        nil
    }

    func validAttributesForMarkedText() -> [NSAttributedString.Key] {
        []
    }

    func firstRect(forCharacterRange range: NSRange, actualRange: NSRangePointer?) -> NSRect {
        guard let caret = textEngine?.textCaretRect(), let window else {
            return .zero
        }
        let local = NSRect(
            x: CGFloat(caret.x),
            y: bounds.height - CGFloat(caret.y) - CGFloat(caret.height),
            width: 1,
            height: CGFloat(caret.height)
        )
        return window.convertToScreen(convert(local, to: nil))
    }

    func characterIndex(for point: NSPoint) -> Int {
        0
    }

    /// Selector-driven editing. Anything not listed is deliberately dropped rather than
    /// falling through to `super`, so a stray command cannot fire a tool shortcut mid-word.
    ///
    /// AppKit names the shift-held variants separately (`…AndModifySelection:`), which is the
    /// whole of how selection reaches the engine from the keyboard: same step, `extend` set.
    override func doCommand(by selector: Selector) {
        guard let engine = textEngine else { return }
        switch selector {
        case #selector(NSStandardKeyBindingResponding.insertNewline(_:)),
             #selector(NSStandardKeyBindingResponding.insertLineBreak(_:)):
            engine.textInsert("\n")
        case #selector(NSStandardKeyBindingResponding.insertTab(_:)):
            engine.textInsert("\t")
        case #selector(NSStandardKeyBindingResponding.deleteBackward(_:)):
            engine.textBackspace()
        case #selector(NSStandardKeyBindingResponding.deleteForward(_:)):
            engine.textDeleteForward()
        // Deleting a word is a word-wide selection and then a delete, which is what it always
        // was — it just could not be said before there was an anchor to hold the far end.
        case #selector(NSStandardKeyBindingResponding.deleteWordBackward(_:)):
            engine.textMoveCaret(.wordLeft, extend: true)
            engine.textBackspace()
        case #selector(NSStandardKeyBindingResponding.deleteWordForward(_:)):
            engine.textMoveCaret(.wordRight, extend: true)
            engine.textDeleteForward()
        case #selector(NSStandardKeyBindingResponding.moveLeft(_:)):
            engine.textMoveCaret(.left)
        case #selector(NSStandardKeyBindingResponding.moveRight(_:)):
            engine.textMoveCaret(.right)
        case #selector(NSStandardKeyBindingResponding.moveWordLeft(_:)),
             #selector(NSStandardKeyBindingResponding.moveWordBackward(_:)):
            engine.textMoveCaret(.wordLeft)
        case #selector(NSStandardKeyBindingResponding.moveWordRight(_:)),
             #selector(NSStandardKeyBindingResponding.moveWordForward(_:)):
            engine.textMoveCaret(.wordRight)
        case #selector(NSStandardKeyBindingResponding.moveUp(_:)):
            engine.textMoveCaret(.up)
        case #selector(NSStandardKeyBindingResponding.moveDown(_:)):
            engine.textMoveCaret(.down)
        case #selector(NSStandardKeyBindingResponding.moveToBeginningOfLine(_:)),
             #selector(NSStandardKeyBindingResponding.moveToLeftEndOfLine(_:)):
            engine.textMoveCaret(.lineStart)
        case #selector(NSStandardKeyBindingResponding.moveToEndOfLine(_:)),
             #selector(NSStandardKeyBindingResponding.moveToRightEndOfLine(_:)):
            engine.textMoveCaret(.lineEnd)
        case #selector(NSStandardKeyBindingResponding.moveToBeginningOfDocument(_:)):
            engine.textMoveCaret(.docStart)
        case #selector(NSStandardKeyBindingResponding.moveToEndOfDocument(_:)):
            engine.textMoveCaret(.docEnd)
        case #selector(NSStandardKeyBindingResponding.moveLeftAndModifySelection(_:)):
            engine.textMoveCaret(.left, extend: true)
        case #selector(NSStandardKeyBindingResponding.moveRightAndModifySelection(_:)):
            engine.textMoveCaret(.right, extend: true)
        case #selector(NSStandardKeyBindingResponding.moveWordLeftAndModifySelection(_:)),
             #selector(NSStandardKeyBindingResponding.moveWordBackwardAndModifySelection(_:)):
            engine.textMoveCaret(.wordLeft, extend: true)
        case #selector(NSStandardKeyBindingResponding.moveWordRightAndModifySelection(_:)),
             #selector(NSStandardKeyBindingResponding.moveWordForwardAndModifySelection(_:)):
            engine.textMoveCaret(.wordRight, extend: true)
        case #selector(NSStandardKeyBindingResponding.moveUpAndModifySelection(_:)):
            engine.textMoveCaret(.up, extend: true)
        case #selector(NSStandardKeyBindingResponding.moveDownAndModifySelection(_:)):
            engine.textMoveCaret(.down, extend: true)
        case #selector(NSStandardKeyBindingResponding.moveToBeginningOfLineAndModifySelection(_:)),
             #selector(NSStandardKeyBindingResponding.moveToLeftEndOfLineAndModifySelection(_:)):
            engine.textMoveCaret(.lineStart, extend: true)
        case #selector(NSStandardKeyBindingResponding.moveToEndOfLineAndModifySelection(_:)),
             #selector(NSStandardKeyBindingResponding.moveToRightEndOfLineAndModifySelection(_:)):
            engine.textMoveCaret(.lineEnd, extend: true)
        case #selector(
            NSStandardKeyBindingResponding.moveToBeginningOfDocumentAndModifySelection(_:)
        ):
            engine.textMoveCaret(.docStart, extend: true)
        case #selector(NSStandardKeyBindingResponding.moveToEndOfDocumentAndModifySelection(_:)):
            engine.textMoveCaret(.docEnd, extend: true)
        case #selector(NSStandardKeyBindingResponding.selectAll(_:)):
            engine.textSelectAll()
        case #selector(NSStandardKeyBindingResponding.cancelOperation(_:)):
            markedTextValue = ""
            engine.commitText()
        default:
            break
        }
    }
}
