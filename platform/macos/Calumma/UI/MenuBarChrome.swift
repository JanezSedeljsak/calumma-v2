import AppKit
import SwiftUI

/// AppKit synthesises a **View** menu for every app; SwiftUI's `.commands` block never
/// declares it, so `CommandGroup(replacing: .toolbar)` can only empty it, not remove it.
/// Deleting it means reaching into `NSApp.mainMenu` after launch — the same kind of shim
/// `WindowAccessor` and `TitleBarZoomOnDoubleClick` already are. Enter Full Screen is
/// re-homed into the Board menu (`⌃⌘F`) so removing View costs no capability.
///
/// Menus are matched by the selectors their items send, not by title: titles are
/// localised by the system and would stop matching in any language but English.
struct MenuBarPruner: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView {
        let view = NSView(frame: .zero)
        DispatchQueue.main.async { Self.removeViewMenu() }
        return view
    }

    /// SwiftUI rebuilds the main menu whenever `.commands` change — a language switch, for
    /// one — which brings View back. Re-pruning on update is a handful of pointer
    /// comparisons over ~6 top-level items.
    func updateNSView(_ nsView: NSView, context: Context) {
        Self.removeViewMenu()
    }

    private static let viewMarkers: Set<Selector> = [
        #selector(NSWindow.toggleToolbarShown(_:)),
        #selector(NSWindow.runToolbarCustomizationPalette(_:)),
        #selector(NSWindow.toggleFullScreen(_:)),
    ]

    /// The Window menu also carries a full-screen-adjacent item on some releases, so a
    /// menu that can miniaturise is Window and is never the one to remove.
    private static let windowMarkers: Set<Selector> = [
        #selector(NSWindow.performMiniaturize(_:)),
        #selector(NSWindow.performZoom(_:)),
    ]

    private static func removeViewMenu() {
        guard let main = NSApp.mainMenu else { return }
        let doomed = main.items.filter { item in
            guard let submenu = item.submenu else { return false }
            let actions = Set(submenu.items.compactMap(\.action))
            return !actions.isDisjoint(with: viewMarkers)
                && actions.isDisjoint(with: windowMarkers)
        }
        for item in doomed {
            main.removeItem(item)
        }
    }
}
