import AppKit

/// The board's pointer, drawn as the tool in hand. A crosshair at the hotspot with the tool's own
/// glyph beside it — the same arrangement Photoshop uses, and for the same reason: the glyph says
/// *what* the click will do, the crosshair says *where* it will land. A glyph sitting on the
/// point would cover the thing you are aiming at.
///
/// Built from `design/icons` through `CalmTool.iconName`, so a tool is the same picture under the
/// pointer as it is in the tools panel. Cursors are cached per tool: an `NSCursor` is rebuilt on
/// every mouse-move otherwise, and this runs on the hot path.
enum ToolCursor {
    /// Tools whose cursor is a system one, and so never reach here: Text is an I-beam, Move is an
    /// arrow or a resize arrow over a guide, and pan / zoom chords own the pointer outright.
    static func cursor(for tool: CalmTool) -> NSCursor? {
        if let cached = cache[tool] {
            return cached
        }
        guard let cursor = build(tool) else { return nil }
        cache[tool] = cursor
        return cursor
    }

    /// Nothing at all, for when the board is already drawing the pointer itself. The engine
    /// rings the brush at the exact stroke size, in document units so it scales with the zoom
    /// (`Document::brush_ring`) — a glyph and a crosshair beside that would be a second pointer
    /// answering a question the ring has already answered better. An empty image rather than
    /// `NSCursor.hide()`, whose hide/unhide counting is one unbalanced call away from a
    /// permanently missing pointer.
    static let ring: NSCursor = {
        let blank = NSImage(size: NSSize(width: 1, height: 1))
        blank.lockFocus()
        NSColor.clear.set()
        NSRect(x: 0, y: 0, width: 1, height: 1).fill()
        blank.unlockFocus()
        return NSCursor(image: blank, hotSpot: .zero)
    }()

    private nonisolated(unsafe) static var cache: [CalmTool: NSCursor] = [:]

    private static let side: CGFloat = 26
    private static let hotspot = CGPoint(x: 5, y: 5)
    /// Half-length of each crosshair arm, and the gap it leaves around the exact point so the
    /// pixel under the hotspot is never painted over.
    private static let armLength: CGFloat = 5
    private static let armGap: CGFloat = 1.5
    private static let glyphSide: CGFloat = 15

    private static func build(_ tool: CalmTool) -> NSCursor? {
        let name = tool.isSelection ? tool.selectionIconName : tool.iconName
        let glyph = SvgIconStore.image(named: name)
        let image = NSImage(size: NSSize(width: side, height: side))
        image.lockFocus()
        defer { image.unlockFocus() }

        // Everything is drawn twice, dark under light, because one colour cannot stay legible
        // over both white paper and black ink — the same problem the brush ring solves the same
        // way (`compose::brush_ring_instances`).
        for pass in Pass.allCases {
            pass.color.set()
            crosshair(inset: pass.spread).fill()
            drawGlyph(glyph, tint: pass.color, spread: pass.spread)
        }
        return NSCursor(image: image, hotSpot: hotspot)
    }

    private enum Pass: CaseIterable {
        case halo
        case ink

        var color: NSColor {
            switch self {
            case .halo: return .black.withAlphaComponent(0.55)
            case .ink: return .white
            }
        }

        /// The halo is the same shape grown by a point on every side; the ink sits inside it.
        var spread: CGFloat {
            switch self {
            case .halo: return 1
            case .ink: return 0
            }
        }
    }

    /// Two bars crossing at the hotspot, with a hole at the centre. In `NSImage` space the origin
    /// is bottom-left, so the hotspot's *y* is measured from the top.
    private static func crosshair(inset spread: CGFloat) -> NSBezierPath {
        let x = hotspot.x
        let y = side - hotspot.y
        let thickness = 1 + spread * 2
        let path = NSBezierPath()
        for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
            let horizontal = dy == 0
            let start = armGap
            let end = armLength + spread
            let rect = NSRect(
                x: horizontal ? x + (dx < 0 ? -end : start) : x - thickness / 2,
                y: horizontal ? y - thickness / 2 : y + (dy < 0 ? -end : start),
                width: horizontal ? end - start : thickness,
                height: horizontal ? thickness : end - start
            )
            path.appendRect(rect)
        }
        return path
    }

    /// The glyph, down and right of the hotspot so it never covers the point being aimed at. The
    /// halo pass stamps it at eight offsets to ring it; the ink pass draws it once on top.
    private static func drawGlyph(_ glyph: NSImage, tint: NSColor, spread: CGFloat) {
        let origin = NSPoint(x: hotspot.x + 4, y: 0)
        let box = NSRect(x: origin.x, y: origin.y, width: glyphSide, height: glyphSide)
        let offsets: [(CGFloat, CGFloat)] =
            spread > 0
            ? [(-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (-1, 1), (1, -1), (1, 1)]
            : [(0, 0)]
        for (dx, dy) in offsets {
            tinted(glyph, tint).draw(
                in: box.offsetBy(dx: dx * spread, dy: dy * spread),
                from: .zero,
                operation: .sourceOver,
                fraction: 1
            )
        }
    }

    /// `SvgIconStore` hands back a template image, which draws as a silhouette in whatever colour
    /// is applied to it — so tinting is a fill through the glyph's own alpha.
    private static func tinted(_ glyph: NSImage, _ color: NSColor) -> NSImage {
        let out = NSImage(size: glyph.size)
        out.lockFocus()
        color.set()
        NSRect(origin: .zero, size: glyph.size).fill()
        glyph.draw(
            in: NSRect(origin: .zero, size: glyph.size),
            from: .zero,
            operation: .destinationIn,
            fraction: 1
        )
        out.unlockFocus()
        return out
    }
}
