import AppKit
import SwiftUI

enum RulerAxis {
    case horizontal
    case vertical
}

/// Figma/Photoshop-style edge ruler in document pixels. Tick *positions* are engine-owned
/// (`Engine.rulerTicksX/Y`, adaptive 1/2/5×10ⁿ spacing) — this view only maps each tick's
/// doc position to a screen offset with the same `doc * zoom + pan` the board itself uses,
/// and draws it. No spacing math lives here.
///
/// It is also where a guide is born: dragging off the strip starts an engine guide drag. The
/// only thing this view contributes is the offset between its own coordinates and the board's
/// (`boardPoint`) — where the guide lands, whether it survives the release, and what it snaps
/// are all decided in `core/src/guide.rs`.
struct RulerView: View {
    @Environment(\.themeColors) private var colors
    let axis: RulerAxis
    let ticks: [RulerTick]
    let zoom: Float
    let pan: Float
    let engine: Engine
    var guidesEnabled = true
    @State private var draggingGuide = false

    static let thickness: CGFloat = 20
    private static let majorTickFraction: CGFloat = 0.55
    private static let minorTickFraction: CGFloat = 0.28
    private static let labelFontSize: CGFloat = 9

    var body: some View {
        Canvas { context, size in
            let font = Font.system(size: Self.labelFontSize, design: .monospaced)
            for tick in ticks {
                let screen = CGFloat(tick.doc * zoom + pan)
                draw(tick: tick, screen: screen, size: size, font: font, in: &context)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(colors.surface)
        .contentShape(Rectangle())
        .gesture(guidesEnabled ? guideDrag : nil)
        .onHover { inside in
            guard guidesEnabled else { return }
            if inside, !draggingGuide {
                guideCursor.set()
            } else if !inside, !draggingGuide {
                NSCursor.arrow.set()
            }
        }
    }

    private var guideDrag: some Gesture {
        DragGesture(minimumDistance: 0)
            .onChanged { value in
                let point = boardPoint(value.location)
                if draggingGuide {
                    engine.updateGuideDrag(x: point.0, y: point.1)
                } else {
                    draggingGuide = true
                    engine.beginGuideDragFromRuler(axis: guideAxis, x: point.0, y: point.1)
                }
            }
            .onEnded { value in
                guard draggingGuide else { return }
                draggingGuide = false
                let point = boardPoint(value.location)
                engine.endGuideDrag(x: point.0, y: point.1)
            }
    }

    private var guideAxis: CalmGuideAxis {
        axis == .horizontal ? .horizontal : .vertical
    }

    private var guideCursor: NSCursor {
        axis == .horizontal ? .resizeUpDown : .resizeLeftRight
    }

    /// A ruler strip is inset from the board by exactly its own thickness on the axis it sits
    /// against, and shares the board's other axis one-for-one — so this is the whole coordinate
    /// bridge. A point still over the strip comes out negative, which is what the engine reads
    /// as "not on the paper" when the drag ends.
    private func boardPoint(_ local: CGPoint) -> (Float, Float) {
        switch axis {
        case .horizontal:
            return (Float(local.x), Float(local.y - Self.thickness))
        case .vertical:
            return (Float(local.x - Self.thickness), Float(local.y))
        }
    }

    private func draw(
        tick: RulerTick,
        screen: CGFloat,
        size: CGSize,
        font: Font,
        in context: inout GraphicsContext
    ) {
        let length = Self.thickness * (tick.major ? Self.majorTickFraction : Self.minorTickFraction)
        let tickColor = colors.textMuted.opacity(tick.major ? 0.9 : 0.4)
        var path = Path()

        switch axis {
        case .horizontal:
            guard screen >= -1, screen <= size.width + 1 else { return }
            path.move(to: CGPoint(x: screen, y: size.height))
            path.addLine(to: CGPoint(x: screen, y: size.height - length))
        case .vertical:
            guard screen >= -1, screen <= size.height + 1 else { return }
            path.move(to: CGPoint(x: size.width, y: screen))
            path.addLine(to: CGPoint(x: size.width - length, y: screen))
        }
        context.stroke(path, with: .color(tickColor), lineWidth: 1)

        guard tick.major else { return }
        let label = context.resolve(
            Text(labelText(for: tick.doc))
                .font(font)
                .foregroundColor(colors.textMuted)
        )
        switch axis {
        case .horizontal:
            context.draw(label, at: CGPoint(x: screen + 3, y: 2), anchor: .topLeading)
        case .vertical:
            context.drawLayer { layer in
                layer.translateBy(x: size.width / 2, y: screen)
                layer.rotate(by: .degrees(-90))
                layer.draw(label, at: .zero, anchor: .center)
            }
        }
    }

    private func labelText(for doc: Float) -> String {
        String(Int(doc.rounded()))
    }
}
