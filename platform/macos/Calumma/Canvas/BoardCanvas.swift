import AppKit
import MetalKit
import SwiftUI

struct BoardCanvas: NSViewRepresentable {
    @EnvironmentObject private var app: AppModel

    func makeCoordinator() -> Coordinator {
        Coordinator(app: app)
    }

    func makeNSView(context: Context) -> MTKView {
        let view = BoardMTKView(frame: .zero, device: MTLCreateSystemDefaultDevice())
        view.delegate = context.coordinator
        view.enableSetNeedsDisplay = false
        view.isPaused = false
        view.preferredFramesPerSecond = 60
        view.framebufferOnly = true
        view.colorPixelFormat = .bgra8Unorm_srgb
        view.clearColor = MTLClearColor(red: 0.05, green: 0.07, blue: 0.09, alpha: 1)
        view.autoResizeDrawable = true
        view.boardCoordinator = context.coordinator
        context.coordinator.attachIfNeeded(view: view)
        return view
    }

    func updateNSView(_ nsView: MTKView, context: Context) {
        context.coordinator.app = app
        context.coordinator.attachIfNeeded(view: nsView)
    }

    static func dismantleNSView(_ nsView: MTKView, coordinator: Coordinator) {
        if let board = nsView as? BoardMTKView {
            board.boardCoordinator = nil
        }
    }

    final class Coordinator: NSObject, MTKViewDelegate {
        var app: AppModel
        private var attached = false

        init(app: AppModel) {
            self.app = app
        }

        func attachIfNeeded(view: MTKView) {
            guard let layer = view.layer else { return }
            let scale = Float(view.window?.backingScaleFactor ?? view.layer?.contentsScale ?? 2)
            layer.contentsScale = CGFloat(scale)
            let width = UInt32(max(view.bounds.width, 1).rounded())
            let height = UInt32(max(view.bounds.height, 1).rounded())
            if !attached {
                let layerPtr = Unmanaged.passUnretained(layer).toOpaque()
                app.engine.attach(layer: layerPtr, width: width, height: height, scale: scale)
                attached = true
                app.engine.fit()
            } else {
                app.engine.resize(width: width, height: height, scale: scale)
            }
        }

        func mtkView(_ view: MTKView, drawableSizeWillChange size: CGSize) {
            let scale = Float(view.window?.backingScaleFactor ?? view.layer?.contentsScale ?? 2)
            view.layer?.contentsScale = CGFloat(scale)
            let width = UInt32(max(view.bounds.width, 1).rounded())
            let height = UInt32(max(view.bounds.height, 1).rounded())
            app.engine.resize(width: width, height: height, scale: scale)
        }

        func draw(in view: MTKView) {
            if !attached {
                attachIfNeeded(view: view)
            }
            app.engine.render()
        }

        func screenPoint(in view: MTKView, event: NSEvent) -> CGPoint {
            let local = view.convert(event.locationInWindow, from: nil)
            return CGPoint(x: local.x, y: view.bounds.height - local.y)
        }
    }
}

final class BoardMTKView: MTKView {
    weak var boardCoordinator: BoardCanvas.Coordinator?
    private var lastDrag: CGPoint?
    private var painting = false
    private var panning = false
    private var trackingArea: NSTrackingArea?

    override var acceptsFirstResponder: Bool { true }

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
        boardCoordinator?.attachIfNeeded(view: self)
        refreshCursor()
    }

    override func viewDidChangeBackingProperties() {
        super.viewDidChangeBackingProperties()
        let scale = window?.backingScaleFactor ?? 2
        layer?.contentsScale = scale
        boardCoordinator?.attachIfNeeded(view: self)
    }

    override func cursorUpdate(with event: NSEvent) {
        refreshCursor()
    }

    override func mouseEntered(with event: NSEvent) {
        refreshCursor()
    }

    override func mouseMoved(with event: NSEvent) {
        refreshCursor()
    }

    override func mouseExited(with event: NSEvent) {
        NSCursor.arrow.set()
    }

    override func flagsChanged(with event: NSEvent) {
        refreshCursor()
        super.flagsChanged(with: event)
    }

    override func mouseDown(with event: NSEvent) {
        guard let coordinator = boardCoordinator else { return }
        let point = coordinator.screenPoint(in: self, event: event)
        if shouldPan(with: event) {
            panning = true
            lastDrag = point
            refreshCursor()
        } else {
            painting = true
            coordinator.app.engine.pointerDown(x: Float(point.x), y: Float(point.y))
            refreshCursor()
        }
    }

    override func mouseDragged(with event: NSEvent) {
        guard let coordinator = boardCoordinator else { return }
        let point = coordinator.screenPoint(in: self, event: event)
        if panning, let lastDrag {
            coordinator.app.engine.pan(dx: Float(point.x - lastDrag.x), dy: Float(point.y - lastDrag.y))
            self.lastDrag = point
            refreshCursor()
        } else if painting {
            coordinator.app.engine.pointerMove(x: Float(point.x), y: Float(point.y))
        }
    }

    override func mouseUp(with event: NSEvent) {
        guard let coordinator = boardCoordinator else { return }
        let point = coordinator.screenPoint(in: self, event: event)
        if painting {
            coordinator.app.engine.pointerUp(x: Float(point.x), y: Float(point.y))
        }
        painting = false
        panning = false
        lastDrag = nil
        refreshCursor()
    }

    override func scrollWheel(with event: NSEvent) {
        guard let coordinator = boardCoordinator else { return }
        let point = coordinator.screenPoint(in: self, event: event)
        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        if flags.contains(.command) || flags.contains(.option) {
            let factor = exp(-Float(event.scrollingDeltaY) * 0.01)
            coordinator.app.engine.zoom(x: Float(point.x), y: Float(point.y), factor: factor)
        } else {
            coordinator.app.engine.pan(dx: Float(event.scrollingDeltaX), dy: -Float(event.scrollingDeltaY))
        }
    }

    override func magnify(with event: NSEvent) {
        guard let coordinator = boardCoordinator else { return }
        let point = coordinator.screenPoint(in: self, event: event)
        coordinator.app.engine.zoom(
            x: Float(point.x),
            y: Float(point.y),
            factor: Float(1 + event.magnification)
        )
    }

    private var spaceHeld: Bool {
        boardCoordinator?.app.spacePan == true
    }

    private func shouldPan(with event: NSEvent) -> Bool {
        if spaceHeld { return true }
        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        return flags.contains(.option) || flags.contains(.command)
    }

    private func refreshCursor() {
        let flags = NSEvent.modifierFlags.intersection(.deviceIndependentFlagsMask)
        let zoomChord = flags.contains(.command) || flags.contains(.option)
        let cursor: NSCursor
        if panning {
            cursor = .closedHand
        } else if spaceHeld {
            cursor = .openHand
        } else if zoomChord {
            cursor = .zoomIn
        } else {
            cursor = .crosshair
        }
        cursor.set()
    }
}
