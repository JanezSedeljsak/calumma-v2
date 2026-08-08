import AppKit
import SwiftUI

private let spaceKeyCode: UInt16 = 49

extension AppModel {
    /// Editor shortcuts, shared by every view that can end up first responder inside the
    /// editor (the catcher below and the Metal board). Clicking the board makes it first
    /// responder, so routing both through here is what keeps shortcuts — and the Space
    /// key-up that ends pan mode — alive after the first stroke.
    /// Returns `true` when the event was consumed.
    @MainActor
    func handleEditorKeyDown(_ event: NSEvent) -> Bool {
        if event.keyCode == spaceKeyCode {
            if !event.isARepeat {
                spacePan = true
                NSCursor.openHand.set()
            }
            return true
        }
        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        let chars = event.charactersIgnoringModifiers ?? ""
        if flags.contains(.command), flags.contains(.shift), chars.lowercased() == "z" {
            engine.redo()
            return true
        }
        if flags.contains(.command), chars.lowercased() == "z" {
            engine.undo()
            return true
        }
        if flags.contains(.command), flags.contains(.shift), chars.lowercased() == "n" {
            engine.addLayer()
            return true
        }
        if flags.contains(.command), event.keyCode == 51 {
            if engine.hasSelection {
                engine.clearSelectionPixels()
            } else {
                engine.clearLayer()
            }
            return true
        }
        if flags.contains(.command), chars.lowercased() == "c" {
            copySelectionOrCanvas()
            return true
        }
        if flags.contains(.command), chars.lowercased() == "x" {
            cutSelection()
            return true
        }
        if flags.contains(.command), chars.lowercased() == "v" {
            pasteFromClipboard()
            return true
        }
        if event.keyCode == 53 {
            engine.deselect()
            return true
        }
        if flags.contains(.command), chars == "=" || chars == "+" {
            engine.stepZoom(in: true)
            return true
        }
        if flags.contains(.command), chars == "-" {
            engine.stepZoom(in: false)
            return true
        }
        if flags.contains(.command), chars.lowercased() == "s" {
            engine.save()
            return true
        }
        if flags.contains(.command), chars.lowercased() == "t" {
            selectTool(.transform)
            return true
        }
        if flags.contains(.command), flags.contains(.option), chars.lowercased() == "l" {
            layersOpen.toggle()
            return true
        }
        switch chars.lowercased() {
        case "p": selectTool(.pen)
        case "l": selectTool(.line)
        case "r": selectTool(.rect)
        case "o": selectTool(.ellipse)
        case "a": selectTool(.arrow)
        case "e": selectTool(.eraser)
        case "m": selectTool(lastSelectTool)
        case "g": selectTool(.bucket)
        case "f": fill.toggle()
        case "0": engine.fit()
        case "[": brushSize = max(1, brushSize - 1)
        case "]": brushSize = min(96, brushSize + 1)
        default: return false
        }
        return true
    }

    @MainActor
    func handleEditorKeyUp(_ event: NSEvent) -> Bool {
        guard event.keyCode == spaceKeyCode else { return false }
        endSpacePan()
        return true
    }

    /// Single exit from temporary pan mode — also called when the app loses focus, so a
    /// Space held across a ⌘-Tab cannot leave the board wedged in pan mode.
    @MainActor
    func endSpacePan() {
        guard spacePan else { return }
        spacePan = false
        NSCursor.crosshair.set()
    }
}

struct ShortcutCatcher: NSViewRepresentable {
    let app: AppModel

    func makeNSView(context: Context) -> NSView {
        let view = KeyView()
        view.app = app
        DispatchQueue.main.async { view.window?.makeFirstResponder(view) }
        return view
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        (nsView as? KeyView)?.app = app
    }

    final class KeyView: NSView {
        nonisolated(unsafe) weak var app: AppModel?

        override var acceptsFirstResponder: Bool { true }

        override func keyDown(with event: NSEvent) {
            let handled = MainActor.assumeIsolated { app?.handleEditorKeyDown(event) ?? false }
            if !handled {
                super.keyDown(with: event)
            }
        }

        override func keyUp(with event: NSEvent) {
            let handled = MainActor.assumeIsolated { app?.handleEditorKeyUp(event) ?? false }
            if !handled {
                super.keyUp(with: event)
            }
        }
    }
}
