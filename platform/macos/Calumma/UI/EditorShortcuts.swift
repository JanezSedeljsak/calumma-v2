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
        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        // Typing owns the keyboard. Only command chords get through, so a bare letter goes
        // into the text layer instead of switching tools.
        if engine.textEditing, !flags.contains(.command) {
            return false
        }
        if event.keyCode == spaceKeyCode {
            if !event.isARepeat {
                spacePan = true
                NSCursor.openHand.set()
            }
            return true
        }
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
        if let step = vectorNudgeStep(event.keyCode), engine.nudgeMoveTarget(x: step.0, y: step.1) {
            return true
        }
        // A selected vector item is what Delete removes; without one the key keeps its old
        // meaning of clearing the selected pixels or the whole layer.
        if event.keyCode == 51 || event.keyCode == 117 {
            if engine.selectedVectorItem != nil {
                engine.deleteSelectedVectorItem()
                return true
            }
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
        if flags.contains(.command), flags.contains(.shift), chars.lowercased() == "i" {
            engine.invertSelection()
            return true
        }
        if flags.contains(.command), chars.lowercased() == "a" {
            engine.selectAll()
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
            engine.toggleTransform()
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
        case "t": selectTool(.text)
        case "3": selectTool(.triangle)
        case "5": selectTool(.pentagon)
        case "e": selectTool(.eraser)
        case "u": selectTool(.blur)
        case "m": selectTool(lastSelectTool)
        case "w": selectTool(.magicWand)
        case "g": selectTool(.bucket)
        case "i": selectTool(.eyedropper)
        case "f": fill.toggle()
        case "s": stroke.toggle()
        case "v": vectorMode.toggle()
        case "0": engine.fit()
        case "[": brushSize = max(1, brushSize - 1)
        case "]": brushSize = min(96, brushSize + 1)
        default: return false
        }
        return true
    }

    /// Arrow keys as one step each, in the engine's step units — the shell names a direction
    /// and never the distance.
    private func vectorNudgeStep(_ keyCode: UInt16) -> (Float, Float)? {
        switch keyCode {
        case 123: return (-1, 0)
        case 124: return (1, 0)
        case 125: return (0, 1)
        case 126: return (0, -1)
        default: return nil
        }
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
