import AppKit
import SwiftUI

struct TitleBarZoomOnDoubleClick: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView {
        let view = NSView(frame: .zero)
        DispatchQueue.main.async { context.coordinator.attach(to: view.window) }
        return view
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        context.coordinator.attach(to: nsView.window)
    }

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    static func dismantleNSView(_ nsView: NSView, coordinator: Coordinator) {
        coordinator.detach()
    }

    final class Coordinator {
        private weak var window: NSWindow?
        private var monitor: Any?

        func attach(to window: NSWindow?) {
            guard let window, window !== self.window else { return }
            detach()
            self.window = window
            monitor = NSEvent.addLocalMonitorForEvents(matching: .leftMouseDown) {
                [weak self] event in
                let zoomed = MainActor.assumeIsolated { self?.zoomIfTitleBar(event) ?? false }
                return zoomed ? nil : event
            }
        }

        func detach() {
            if let monitor {
                NSEvent.removeMonitor(monitor)
            }
            monitor = nil
            window = nil
        }

        deinit {
            if let monitor {
                NSEvent.removeMonitor(monitor)
            }
        }

        @MainActor
        private func zoomIfTitleBar(_ event: NSEvent) -> Bool {
            guard event.clickCount == 2,
                  let window,
                  event.window === window,
                  isTitleBar(event, in: window)
            else {
                return false
            }
            window.zoom(nil)
            return true
        }

        @MainActor
        private func isTitleBar(_ event: NSEvent, in window: NSWindow) -> Bool {
            let height = window.frame.height - window.contentLayoutRect.height
            guard height > 0 else { return false }
            let point = event.locationInWindow
            guard point.y >= window.frame.height - height else { return false }
            let hit = window.contentView?.hitTest(point)
            return hit == nil || hit is NSVisualEffectView || hit === window.contentView
        }
    }
}
