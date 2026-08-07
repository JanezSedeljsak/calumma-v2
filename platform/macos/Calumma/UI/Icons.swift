import SwiftUI

enum AppIcon {
    static func settings(color: Color) -> some View {
        IconCanvas(color: color) { ctx, rect in
            let inset = rect.insetBy(dx: 4, dy: 4)
            ctx.stroke(
                Path(ellipseIn: inset),
                with: .color(color),
                style: StrokeStyle(lineWidth: 1.8)
            )
            let hub = rect.insetBy(dx: 7, dy: 7)
            ctx.stroke(
                Path(ellipseIn: hub),
                with: .color(color),
                style: StrokeStyle(lineWidth: 1.6)
            )
        }
    }

    static func plus(color: Color) -> some View {
        IconCanvas(color: color) { ctx, rect in
            let mid = CGPoint(x: rect.midX, y: rect.midY)
            ctx.stroke(
                Path { p in
                    p.move(to: CGPoint(x: mid.x, y: rect.minY + 3))
                    p.addLine(to: CGPoint(x: mid.x, y: rect.maxY - 3))
                    p.move(to: CGPoint(x: rect.minX + 3, y: mid.y))
                    p.addLine(to: CGPoint(x: rect.maxX - 3, y: mid.y))
                },
                with: .color(color),
                style: StrokeStyle(lineWidth: 2, lineCap: .round)
            )
        }
    }

    static func pen(color: Color) -> some View {
        IconCanvas(color: color) { ctx, rect in
            var path = Path()
            path.move(to: CGPoint(x: rect.minX + 3, y: rect.maxY - 3))
            path.addLine(to: CGPoint(x: rect.minX + 7, y: rect.maxY - 3))
            path.addLine(to: CGPoint(x: rect.maxX - 4, y: rect.minY + 7))
            path.addLine(to: CGPoint(x: rect.maxX - 8, y: rect.minY + 3))
            path.closeSubpath()
            ctx.stroke(path, with: .color(color), style: StrokeStyle(lineWidth: 1.8, lineJoin: .round))
        }
    }

    static func shape(color: Color) -> some View {
        IconCanvas(color: color) { ctx, rect in
            let inset = rect.insetBy(dx: 4, dy: 4)
            ctx.stroke(
                Path(roundedRect: inset, cornerRadius: 3),
                with: .color(color),
                style: StrokeStyle(lineWidth: 1.8)
            )
        }
    }

    static func image(color: Color) -> some View {
        IconCanvas(color: color) { ctx, rect in
            let inset = rect.insetBy(dx: 3, dy: 3)
            ctx.stroke(
                Path(roundedRect: inset, cornerRadius: 4),
                with: .color(color),
                style: StrokeStyle(lineWidth: 1.8)
            )
            ctx.fill(
                Path(ellipseIn: CGRect(x: inset.minX + 3, y: inset.minY + 3, width: 3, height: 3)),
                with: .color(color)
            )
        }
    }

    static func line(color: Color) -> some View {
        IconCanvas(color: color) { ctx, rect in
            ctx.stroke(
                Path { p in
                    p.move(to: CGPoint(x: rect.minX + 4, y: rect.maxY - 5))
                    p.addLine(to: CGPoint(x: rect.maxX - 4, y: rect.minY + 5))
                },
                with: .color(color),
                style: StrokeStyle(lineWidth: 2, lineCap: .round)
            )
        }
    }

    static func ellipse(color: Color) -> some View {
        IconCanvas(color: color) { ctx, rect in
            ctx.stroke(
                Path(ellipseIn: rect.insetBy(dx: 4, dy: 5)),
                with: .color(color),
                style: StrokeStyle(lineWidth: 1.8)
            )
        }
    }

    static func arrow(color: Color) -> some View {
        IconCanvas(color: color) { ctx, rect in
            ctx.stroke(
                Path { p in
                    p.move(to: CGPoint(x: rect.minX + 4, y: rect.maxY - 5))
                    p.addLine(to: CGPoint(x: rect.maxX - 5, y: rect.minY + 6))
                    p.move(to: CGPoint(x: rect.maxX - 5, y: rect.minY + 6))
                    p.addLine(to: CGPoint(x: rect.maxX - 10, y: rect.minY + 7))
                    p.move(to: CGPoint(x: rect.maxX - 5, y: rect.minY + 6))
                    p.addLine(to: CGPoint(x: rect.maxX - 6, y: rect.minY + 12))
                },
                with: .color(color),
                style: StrokeStyle(lineWidth: 1.8, lineCap: .round, lineJoin: .round)
            )
        }
    }

    static func ai(color: Color) -> some View {
        IconCanvas(color: color) { ctx, rect in
            let sparkles = [
                CGRect(x: rect.midX - 1.5, y: rect.minY + 2.5, width: 3, height: 3),
                CGRect(x: rect.minX + 3, y: rect.midY - 1, width: 2.4, height: 2.4),
                CGRect(x: rect.maxX - 5.5, y: rect.midY + 1, width: 2.4, height: 2.4),
                CGRect(x: rect.midX - 1.2, y: rect.maxY - 5.5, width: 2.4, height: 2.4),
            ]
            for spark in sparkles {
                ctx.fill(Path(ellipseIn: spark), with: .color(color))
            }
            ctx.stroke(
                Path { p in
                    p.move(to: CGPoint(x: rect.midX, y: rect.minY + 6))
                    p.addLine(to: CGPoint(x: rect.midX, y: rect.maxY - 6))
                    p.move(to: CGPoint(x: rect.minX + 6, y: rect.midY))
                    p.addLine(to: CGPoint(x: rect.maxX - 6, y: rect.midY))
                },
                with: .color(color),
                style: StrokeStyle(lineWidth: 1.4, lineCap: .round)
            )
        }
    }

    static func eye(color: Color, open: Bool = true) -> some View {
        IconCanvas(color: color) { ctx, rect in
            let lid = Path(ellipseIn: rect.insetBy(dx: 3, dy: 6))
            ctx.stroke(lid, with: .color(color), style: StrokeStyle(lineWidth: 1.6))
            if open {
                let pupil = Path(ellipseIn: CGRect(
                    x: rect.midX - 2.2,
                    y: rect.midY - 2.2,
                    width: 4.4,
                    height: 4.4
                ))
                ctx.fill(pupil, with: .color(color))
            } else {
                ctx.stroke(
                    Path { p in
                        p.move(to: CGPoint(x: rect.minX + 4, y: rect.maxY - 5))
                        p.addLine(to: CGPoint(x: rect.maxX - 4, y: rect.minY + 5))
                    },
                    with: .color(color),
                    style: StrokeStyle(lineWidth: 1.6, lineCap: .round)
                )
            }
        }
    }

    static func trash(color: Color) -> some View {
        IconCanvas(color: color) { ctx, rect in
            ctx.stroke(
                Path { p in
                    p.move(to: CGPoint(x: rect.minX + 5, y: rect.minY + 6))
                    p.addLine(to: CGPoint(x: rect.maxX - 5, y: rect.minY + 6))
                    p.move(to: CGPoint(x: rect.minX + 7, y: rect.minY + 6))
                    p.addLine(to: CGPoint(x: rect.minX + 7, y: rect.maxY - 4))
                    p.addLine(to: CGPoint(x: rect.maxX - 7, y: rect.maxY - 4))
                    p.addLine(to: CGPoint(x: rect.maxX - 7, y: rect.minY + 6))
                    p.move(to: CGPoint(x: rect.minX + 8, y: rect.minY + 4))
                    p.addLine(to: CGPoint(x: rect.maxX - 8, y: rect.minY + 4))
                },
                with: .color(color),
                style: StrokeStyle(lineWidth: 1.5, lineCap: .round, lineJoin: .round)
            )
        }
    }
}

private struct IconCanvas: View {
    let color: Color
    let draw: (inout GraphicsContext, CGRect) -> Void

    var body: some View {
        Canvas { context, size in
            var ctx = context
            draw(&ctx, CGRect(origin: .zero, size: size))
        }
        .frame(width: 18, height: 18)
        .accessibilityHidden(true)
    }
}
