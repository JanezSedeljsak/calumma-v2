import AppKit
import SwiftUI

private let spaceKeyCode: UInt16 = 49
/// Return and the keypad's Enter — the same key as far as anything here is concerned.
private let returnKeyCodes: Set<UInt16> = [36, 76]

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
        // Return leaves transform and touches nothing else — same layer, same selection, same
        // tool, and the transform itself stays on the layer (it is a `LayerTransform`, so there
        // is no commit/cancel split to make). `Esc` below is the wider exit: `Document::deselect`
        // drops the selection with it. Photoshop's polarity, and why both are worth having.
        if returnKeyCodes.contains(event.keyCode), engine.state.transformActive {
            engine.exitTransform()
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
            toggleMoveTransform()
            return true
        }
        if flags.contains(.command), flags.contains(.option), chars.lowercased() == "l" {
            layersOpen.toggle()
            return true
        }
        if flags.contains(.command), flags.contains(.option), chars.lowercased() == "g" {
            clipActiveLayerDown()
            return true
        }
        let key = chars.lowercased()
        // Tool keys come from `CalmTool.byKey` rather than a switch of their own, so the key
        // that switches a tool is the same one its tooltip prints. What is left below is the
        // keys that are not tools.
        if let picked = CalmTool.forShortcut(key) {
            // The marquee key names a family; which of the three it lands on is the last one
            // used. Every other key names exactly one tool.
            pickTool(picked == .selectRect ? lastSelectTool : picked)
            return true
        }
        switch key {
        case "f": fill.toggle()
        case "s": stroke.toggle()
        case "v": vectorMode.toggle()
        case "0": engine.fit()
        case "[":
            if tool.takesEyedropperRadius {
                if eyedropperRadius > Engine.eyedropperRadiusMin {
                    eyedropperRadius -= 1
                }
            } else {
                brushSize = Engine.brushSizeStep(brushSize, increase: false)
            }
        case "]":
            if tool.takesEyedropperRadius {
                if eyedropperRadius < Engine.eyedropperRadiusMax {
                    eyedropperRadius += 1
                }
            } else {
                brushSize = Engine.brushSizeStep(brushSize, increase: true)
            }
        default: return false
        }
        return true
    }

    /// Clip to Layer Below on the *active* layer, which is the only thing a chord can mean — the
    /// card's button names a row, a key press has only the layer you are working on. Refused the
    /// same way the greyed-out button is, since both ask the engine.
    @MainActor
    private func clipActiveLayerDown() {
        let index = Int(engine.state.activeLayer)
        guard engine.canClipLayerDown(index: index) else { return }
        engine.clipLayerDown(index)
    }

    /// A shortcut asks the same question the tools panel does before it switches: a key that
    /// selects a tool the active layer refuses would put the user in a state the panel already
    /// shows as unusable. The refusal is said out loud rather than swallowed, because unlike a
    /// greyed-out button a key press has nothing to look at.
    @MainActor
    private func pickTool(_ next: CalmTool) {
        let block = engine.toolBlock(next)
        guard !block.blocks else {
            engine.toolBlockNotice = block
            return
        }
        selectTool(next)
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
    ///
    /// Puts no cursor back itself: `spacePan` is published, so dropping it re-runs
    /// `BoardCanvas.updateNSView` and the board picks the cursor for whatever tool is in hand.
    /// Setting one here used to be harmless when every tool shared the crosshair; now it would
    /// be the wrong picture until the next mouse-move.
    @MainActor
    func endSpacePan() {
        guard spacePan else { return }
        spacePan = false
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
