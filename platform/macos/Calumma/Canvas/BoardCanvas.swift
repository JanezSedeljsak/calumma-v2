import AppKit
import MetalKit
import SwiftUI

struct BoardCanvas: NSViewRepresentable {
    @EnvironmentObject private var app: AppModel

    func makeCoordinator() -> Coordinator {
        Coordinator(engine: app.engine)
    }

    func makeNSView(context: Context) -> MTKView {
        let view = BoardMTKView(frame: .zero, device: MTLCreateSystemDefaultDevice())
        view.app = app
        view.delegate = context.coordinator
        view.enableSetNeedsDisplay = false
        view.isPaused = false
        view.preferredFramesPerSecond = NSScreen.main?.maximumFramesPerSecond ?? 60
        view.framebufferOnly = true
        view.colorPixelFormat = .bgra8Unorm_srgb
        view.clearColor = MTLClearColor(red: 0.039, green: 0.047, blue: 0.059, alpha: 1)
        view.autoResizeDrawable = true
        view.boardCoordinator = context.coordinator
        view.wantsLayer = true
        view.layer?.cornerRadius = Tokens.Radius.island
        view.layer?.masksToBounds = true
        context.coordinator.attachIfNeeded(view: view)
        return view
    }

    func updateNSView(_ nsView: MTKView, context: Context) {
        context.coordinator.spacePan = app.spacePan
        context.coordinator.attachIfNeeded(view: nsView)
        if let board = nsView as? BoardMTKView {
            board.app = app
            board.refreshCursor()
        }
    }

    static func dismantleNSView(_ nsView: MTKView, coordinator: Coordinator) {
        if let board = nsView as? BoardMTKView {
            board.boardCoordinator = nil
            board.app = nil
        }
    }

    private struct Surface: Equatable {
        var width: UInt32
        var height: UInt32
        var scale: Float
    }

    final class Coordinator: NSObject, MTKViewDelegate {
        let engine: Engine
        var spacePan = false
        private var attached = false
        private var lastSurface: Surface?

        init(engine: Engine) {
            self.engine = engine
        }

        func attachIfNeeded(view: MTKView) {
            guard let layer = view.layer else { return }
            let scale = Float(view.window?.backingScaleFactor ?? view.layer?.contentsScale ?? 2)
            layer.contentsScale = CGFloat(scale)
            let next = currentSurface(of: view, scale: scale)
            if !attached {
                let layerPtr = Unmanaged.passUnretained(layer).toOpaque()
                engine.attach(
                    layer: layerPtr,
                    width: next.width,
                    height: next.height,
                    scale: scale
                )
                attached = true
                lastSurface = next
                engine.fitToScreen()
            } else {
                resize(to: next)
            }
        }

        func mtkView(_ view: MTKView, drawableSizeWillChange size: CGSize) {
            let scale = Float(view.window?.backingScaleFactor ?? view.layer?.contentsScale ?? 2)
            view.layer?.contentsScale = CGFloat(scale)
            resize(to: currentSurface(of: view, scale: scale))
        }

        private func currentSurface(of view: MTKView, scale: Float) -> Surface {
            Surface(
                width: UInt32(max(view.bounds.width, 1).rounded()),
                height: UInt32(max(view.bounds.height, 1).rounded()),
                scale: scale
            )
        }

        /// SwiftUI re-runs `updateNSView` on every published engine state, and a resize
        /// publishes state of its own — so a resize per update would loop. Only a real size
        /// change reaches the engine.
        private func resize(to next: Surface) {
            guard lastSurface != next else { return }
            lastSurface = next
            engine.resize(width: next.width, height: next.height, scale: next.scale)
        }

        func draw(in view: MTKView) {
            if !attached {
                attachIfNeeded(view: view)
            }
            engine.flushPendingState()
            engine.render()
        }

        func screenPoint(in view: MTKView, event: NSEvent) -> CGPoint {
            let local = view.convert(event.locationInWindow, from: nil)
            return CGPoint(x: local.x, y: view.bounds.height - local.y)
        }
    }
}

private let middleButton = 2

final class BoardMTKView: MTKView {
    weak var boardCoordinator: BoardCanvas.Coordinator?
    /// Clicking the board makes it first responder, so it has to serve the same editor
    /// shortcuts the catcher does — otherwise the keyboard goes dead after the first stroke
    /// and a held Space never gets its key-up, wedging the board in pan mode.
    nonisolated(unsafe) weak var app: AppModel?
    var markedTextValue = ""
    /// The guide under the pointer, refreshed on every move so `refreshCursor` — which is also
    /// called from places that have no event to read a position from — can offer a grab without
    /// asking the engine again.
    private var hoveredGuideAxis: CalmGuideAxis?
    private var lastDrag: CGPoint?
    private var painting = false
    private var panning = false
    private var trackingArea: NSTrackingArea?

    override var acceptsFirstResponder: Bool { true }

    /// While a text layer is open, the keyboard belongs to it — `interpretKeyEvents` turns
    /// the event into `insertText` or a `doCommand` selector (see `BoardTextInput`). Only
    /// command chords still reach the editor shortcuts, so ⌘Z and ⌘S keep working mid-word
    /// while a bare `p` types a p instead of selecting the pen.
    override func keyDown(with event: NSEvent) {
        if isTypingOnBoard {
            let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
            if flags.contains(.command) {
                let handled = MainActor.assumeIsolated { app?.handleEditorKeyDown(event) ?? false }
                if !handled {
                    super.keyDown(with: event)
                }
                return
            }
            interpretKeyEvents([event])
            return
        }
        let handled = MainActor.assumeIsolated { app?.handleEditorKeyDown(event) ?? false }
        if !handled {
            super.keyDown(with: event)
        }
    }

    override func keyUp(with event: NSEvent) {
        if isTypingOnBoard {
            return
        }
        let handled = MainActor.assumeIsolated { app?.handleEditorKeyUp(event) ?? false }
        if !handled {
            super.keyUp(with: event)
        }
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let trackingArea {
            removeTrackingArea(trackingArea)
        }
        let options: NSTrackingArea.Options = [
            .activeInKeyWindow,
            .mouseMoved,
            .mouseEnteredAndExited,
            .inVisibleRect,
            .cursorUpdate,
        ]
        let area = NSTrackingArea(rect: .zero, options: options, owner: self, userInfo: nil)
        addTrackingArea(area)
        trackingArea = area
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        let scale = window?.backingScaleFactor ?? 2
        layer?.contentsScale = scale
        syncRefreshRate()
        boardCoordinator?.attachIfNeeded(view: self)
        refreshCursor()
    }

    override func viewDidChangeBackingProperties() {
        super.viewDidChangeBackingProperties()
        let scale = window?.backingScaleFactor ?? 2
        layer?.contentsScale = scale
        syncRefreshRate()
        boardCoordinator?.attachIfNeeded(view: self)
    }

    /// A ProMotion display runs at 120Hz; pinned to 60 every pan looks like it is dropping
    /// every other frame. Follow whichever screen the window is actually on.
    private func syncRefreshRate() {
        let screen = window?.screen ?? NSScreen.main
        preferredFramesPerSecond = screen?.maximumFramesPerSecond ?? 60
    }

    override func cursorUpdate(with event: NSEvent) {
        refreshCursor()
    }

    override func mouseEntered(with event: NSEvent) {
        refreshCursor()
    }

    override func mouseMoved(with event: NSEvent) {
        updateHoveredGuide(with: event)
        refreshCursor()
        updateEyedropper(with: event)
    }

    override func mouseExited(with event: NSEvent) {
        hoveredGuideAxis = nil
        NSCursor.arrow.set()
        MainActor.assumeIsolated { app?.clearEyedropperLoupe() }
    }

    /// Shift constrains the shape being dragged, so the engine has to hear about the key
    /// itself — waiting for the next mouse-move would leave the board showing a rectangle
    /// while the user is already holding Shift.
    override func flagsChanged(with event: NSEvent) {
        refreshCursor()
        if painting {
            boardCoordinator?.engine.setShift(event.modifierFlags.contains(.shift))
        }
        super.flagsChanged(with: event)
    }

    override func mouseDown(with event: NSEvent) {
        guard let coordinator = boardCoordinator else { return }
        let point = coordinator.screenPoint(in: self, event: event)
        if shouldPan(with: event) {
            panning = true
            lastDrag = point
            refreshCursor()
        } else if MainActor.assumeIsolated({ app?.tool == .eyedropper }) {
            if let color = coordinator.engine.pickColor(x: Float(point.x), y: Float(point.y)) {
                let local = convert(event.locationInWindow, from: nil)
                let uiPoint = CGPoint(x: local.x, y: bounds.height - local.y)
                MainActor.assumeIsolated {
                    app?.applyEyedropperSample(color, at: uiPoint)
                }
            }
            refreshCursor()
        } else if MainActor.assumeIsolated({ app?.tool == .text }) {
            // A click with the Text tool opens or re-enters a layer, so the board has to own
            // the keyboard before the next keystroke and the layers panel has to hear about
            // the new layer straight away.
            window?.makeFirstResponder(self)
            markedTextValue = ""
            coordinator.engine.pointerDown(x: Float(point.x), y: Float(point.y))
            coordinator.engine.refreshLayers()
            refreshCursor()
        } else {
            painting = true
            coordinator.engine.setShift(event.modifierFlags.contains(.shift))
            coordinator.engine.pointerDown(x: Float(point.x), y: Float(point.y))
            refreshCursor()
        }
    }

    override func mouseDragged(with event: NSEvent) {
        guard let coordinator = boardCoordinator else { return }
        let point = coordinator.screenPoint(in: self, event: event)
        if panning, let lastDrag {
            coordinator.engine.pan(dx: Float(point.x - lastDrag.x), dy: Float(point.y - lastDrag.y))
            self.lastDrag = point
            refreshCursor()
        } else if MainActor.assumeIsolated({ app?.tool == .eyedropper }) {
            updateEyedropper(with: event)
        } else if painting {
            coordinator.engine.setShift(event.modifierFlags.contains(.shift))
            coordinator.engine.pointerMove(x: Float(point.x), y: Float(point.y))
        }
    }

    override func mouseUp(with event: NSEvent) {
        guard let coordinator = boardCoordinator else { return }
        let point = coordinator.screenPoint(in: self, event: event)
        let wasPanning = panning
        if painting {
            coordinator.engine.setShift(event.modifierFlags.contains(.shift))
            coordinator.engine.pointerUp(x: Float(point.x), y: Float(point.y))
        }
        painting = false
        panning = false
        lastDrag = nil
        if wasPanning {
            coordinator.engine.endCameraMotion()
        }
        refreshCursor()
    }

    override func otherMouseDown(with event: NSEvent) {
        guard event.buttonNumber == middleButton, let coordinator = boardCoordinator else {
            super.otherMouseDown(with: event)
            return
        }
        panning = true
        lastDrag = coordinator.screenPoint(in: self, event: event)
        refreshCursor()
    }

    override func otherMouseDragged(with event: NSEvent) {
        guard event.buttonNumber == middleButton,
              panning,
              let coordinator = boardCoordinator,
              let lastDrag
        else {
            super.otherMouseDragged(with: event)
            return
        }
        let point = coordinator.screenPoint(in: self, event: event)
        coordinator.engine.pan(dx: Float(point.x - lastDrag.x), dy: Float(point.y - lastDrag.y))
        self.lastDrag = point
        refreshCursor()
    }

    override func otherMouseUp(with event: NSEvent) {
        guard event.buttonNumber == middleButton else {
            super.otherMouseUp(with: event)
            return
        }
        let wasPanning = panning
        panning = false
        lastDrag = nil
        if wasPanning, let coordinator = boardCoordinator {
            coordinator.engine.endCameraMotion()
        }
        refreshCursor()
    }

    override func scrollWheel(with event: NSEvent) {
        guard let coordinator = boardCoordinator else { return }
        let point = coordinator.screenPoint(in: self, event: event)
        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        let precise = event.hasPreciseScrollingDeltas
        if flags.contains(.command) || flags.contains(.option) {
            coordinator.engine.zoomScroll(
                x: Float(point.x),
                y: Float(point.y),
                delta: Float(event.scrollingDeltaY),
                precise: precise
            )
        } else {
            coordinator.engine.panScroll(
                dx: Float(event.scrollingDeltaX),
                dy: Float(event.scrollingDeltaY),
                precise: precise
            )
        }
    }

    override func magnify(with event: NSEvent) {
        guard let coordinator = boardCoordinator else { return }
        let point = coordinator.screenPoint(in: self, event: event)
        coordinator.engine.zoom(
            x: Float(point.x),
            y: Float(point.y),
            factor: Float(1 + event.magnification)
        )
    }

    /// Read straight off the model rather than the coordinator copy — the copy only
    /// refreshes on the next SwiftUI update, which can land after the mouse-down that
    /// was supposed to pan.
    private var spaceHeld: Bool {
        if let app {
            return MainActor.assumeIsolated { app.spacePan }
        }
        return boardCoordinator?.spacePan == true
    }

    private func shouldPan(with event: NSEvent) -> Bool {
        if spaceHeld { return true }
        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        return flags.contains(.option) || flags.contains(.command)
    }

    private func updateHoveredGuide(with event: NSEvent) {
        guard let coordinator = boardCoordinator,
              MainActor.assumeIsolated({ app?.tool == .move }),
              !panning,
              !spaceHeld
        else {
            hoveredGuideAxis = nil
            return
        }
        let point = coordinator.screenPoint(in: self, event: event)
        hoveredGuideAxis = coordinator.engine.guideAxis(atX: Float(point.x), y: Float(point.y))
    }

    private func updateEyedropper(with event: NSEvent) {
        guard let coordinator = boardCoordinator else { return }
        let eyedropper = MainActor.assumeIsolated { app?.tool == .eyedropper }
        guard eyedropper, !panning, !spaceHeld else {
            MainActor.assumeIsolated { app?.clearEyedropperLoupe() }
            return
        }
        let point = coordinator.screenPoint(in: self, event: event)
        let local = convert(event.locationInWindow, from: nil)
        let uiPoint = CGPoint(x: local.x, y: bounds.height - local.y)
        if let color = coordinator.engine.sampleColor(x: Float(point.x), y: Float(point.y)) {
            MainActor.assumeIsolated {
                app?.applyEyedropperSample(color, at: uiPoint)
            }
        } else {
            MainActor.assumeIsolated { app?.clearEyedropperLoupe() }
        }
    }

    func refreshCursor() {
        let flags = NSEvent.modifierFlags.intersection(.deviceIndependentFlagsMask)
        let zoomChord = flags.contains(.command) || flags.contains(.option)
        let cursor: NSCursor
        if panning {
            cursor = .closedHand
        } else if spaceHeld {
            cursor = .openHand
        } else if zoomChord {
            cursor = .zoomIn
        } else if MainActor.assumeIsolated({ app?.tool == .text }) {
            cursor = .iBeam
        } else if MainActor.assumeIsolated({ app?.tool == .move }) {
            switch hoveredGuideAxis {
            case .horizontal: cursor = .resizeUpDown
            case .vertical: cursor = .resizeLeftRight
            case nil: cursor = .arrow
            }
        } else {
            cursor = .crosshair
        }
        cursor.set()
    }
}
