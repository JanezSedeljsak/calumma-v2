import SwiftUI

enum RulerAxis {
    case horizontal
    case vertical
}

/// Figma/Photoshop-style edge ruler in document pixels. Tick *positions* are engine-owned
/// (`Engine.rulerTicksX/Y`, adaptive 1/2/5×10ⁿ spacing) — this view only maps each tick's
/// doc position to a screen offset with the same `doc * zoom + pan` the board itself uses,
/// and draws it. No spacing math lives here.
struct RulerView: View {
    @Environment(\.themeColors) private var colors
    let axis: RulerAxis
    let ticks: [RulerTick]
    let zoom: Float
    let pan: Float

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
