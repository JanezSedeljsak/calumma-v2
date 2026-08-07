import SwiftUI

private enum IconMetrics {
    static let stroke: CGFloat = 1.6
    static let margin: CGFloat = 4
}

private func rotatedPolygon(center: CGPoint, local: [CGPoint], angle: CGFloat) -> Path {
    let points = local.map { point in
        CGPoint(
            x: center.x + point.x * cos(angle) - point.y * sin(angle),
            y: center.y + point.x * sin(angle) + point.y * cos(angle)
        )
    }
    var path = Path()
    guard let first = points.first else { return path }
    path.move(to: first)
    for point in points.dropFirst() {
        path.addLine(to: point)
    }
    path.closeSubpath()
    return path
}

enum AppIcon {
    static func settings(color: Color) -> some View {
        IconCanvas(color: color) { ctx, rect in
            let center = CGPoint(x: rect.midX, y: rect.midY)
            let teeth = 8
            let outerRadius: CGFloat = 8
            let innerRadius: CGFloat = 6
            var gear = Path()
            for i in 0..<(teeth * 2) {
                let angle = CGFloat(i) / CGFloat(teeth * 2) * 2 * .pi
                let radius = i % 2 == 0 ? outerRadius : innerRadius
                let point = CGPoint(
                    x: center.x + radius * cos(angle),
                    y: center.y + radius * sin(angle)
                )
                if i == 0 {
                    gear.move(to: point)
                } else {
                    gear.addLine(to: point)
                }
            }
            gear.closeSubpath()
            ctx.stroke(gear, with: .color(color), style: StrokeStyle(lineWidth: IconMetrics.stroke, lineJoin: .round))
            let hub = CGRect(x: center.x - 2.2, y: center.y - 2.2, width: 4.4, height: 4.4)
            ctx.stroke(Path(ellipseIn: hub), with: .color(color), style: StrokeStyle(lineWidth: IconMetrics.stroke))
        }
    }

    static func plus(color: Color) -> some View {
        IconCanvas(color: color) { ctx, rect in
            let mid = CGPoint(x: rect.midX, y: rect.midY)
            ctx.stroke(
                Path { p in
                    p.move(to: CGPoint(x: mid.x, y: rect.minY + IconMetrics.margin))
                    p.addLine(to: CGPoint(x: mid.x, y: rect.maxY - IconMetrics.margin))
                    p.move(to: CGPoint(x: rect.minX + IconMetrics.margin, y: mid.y))
                    p.addLine(to: CGPoint(x: rect.maxX - IconMetrics.margin, y: mid.y))
                },
                with: .color(color),
                style: StrokeStyle(lineWidth: IconMetrics.stroke, lineCap: .round)
            )
        }
    }

    static func pen(color: Color) -> some View {
        IconCanvas(color: color) { ctx, rect in
            let center = CGPoint(x: rect.midX, y: rect.midY)
            let body = rotatedPolygon(
                center: center,
                local: [
                    CGPoint(x: -6, y: -2),
                    CGPoint(x: 3, y: -2),
                    CGPoint(x: 7, y: 0),
                    CGPoint(x: 3, y: 2),
                    CGPoint(x: -6, y: 2),
                ],
                angle: -.pi / 4
            )
            ctx.fill(body, with: .color(color.opacity(0.85)))
            ctx.stroke(body, with: .color(color), style: StrokeStyle(lineWidth: IconMetrics.stroke, lineJoin: .round))
        }
    }

    static func shape(color: Color) -> some View {
        IconCanvas(color: color) { ctx, rect in
            let inset = rect.insetBy(dx: IconMetrics.margin, dy: IconMetrics.margin)
            ctx.stroke(
                Path(roundedRect: inset, cornerRadius: 3),
                with: .color(color),
                style: StrokeStyle(lineWidth: IconMetrics.stroke)
            )
        }
    }

    static func image(color: Color) -> some View {
        IconCanvas(color: color) { ctx, rect in
            let inset = rect.insetBy(dx: 3, dy: 3)
            ctx.stroke(
                Path(roundedRect: inset, cornerRadius: 4),
                with: .color(color),
                style: StrokeStyle(lineWidth: IconMetrics.stroke)
            )
            ctx.fill(
                Path(ellipseIn: CGRect(x: inset.minX + 3, y: inset.minY + 3, width: 3, height: 3)),
                with: .color(color)
            )
        }
    }

    static func eraser(color: Color) -> some View {
        IconCanvas(color: color) { ctx, rect in
            let body = rotatedPolygon(
                center: CGPoint(x: rect.midX, y: rect.midY),
                local: [
                    CGPoint(x: -6.5, y: -3.5),
                    CGPoint(x: 6.5, y: -3.5),
                    CGPoint(x: 6.5, y: 3.5),
                    CGPoint(x: -6.5, y: 3.5),
                ],
                angle: .pi / 4
            )
            ctx.fill(body, with: .color(color.opacity(0.85)))
            ctx.stroke(body, with: .color(color), style: StrokeStyle(lineWidth: IconMetrics.stroke, lineJoin: .round))
        }
    }

    static func fitToView(color: Color) -> some View {
        IconCanvas(color: color) { ctx, rect in
            let inset = rect.insetBy(dx: IconMetrics.margin, dy: IconMetrics.margin)
            let arm: CGFloat = 4
            var corners = Path()
            corners.move(to: CGPoint(x: inset.minX, y: inset.minY + arm))
            corners.addLine(to: CGPoint(x: inset.minX, y: inset.minY))
            corners.addLine(to: CGPoint(x: inset.minX + arm, y: inset.minY))
            corners.move(to: CGPoint(x: inset.maxX - arm, y: inset.minY))
            corners.addLine(to: CGPoint(x: inset.maxX, y: inset.minY))
            corners.addLine(to: CGPoint(x: inset.maxX, y: inset.minY + arm))
            corners.move(to: CGPoint(x: inset.maxX, y: inset.maxY - arm))
            corners.addLine(to: CGPoint(x: inset.maxX, y: inset.maxY))
            corners.addLine(to: CGPoint(x: inset.maxX - arm, y: inset.maxY))
            corners.move(to: CGPoint(x: inset.minX + arm, y: inset.maxY))
            corners.addLine(to: CGPoint(x: inset.minX, y: inset.maxY))
            corners.addLine(to: CGPoint(x: inset.minX, y: inset.maxY - arm))
            ctx.stroke(corners, with: .color(color), style: StrokeStyle(lineWidth: IconMetrics.stroke, lineCap: .round, lineJoin: .round))
        }
    }

    static func line(color: Color) -> some View {
        IconCanvas(color: color) { ctx, rect in
            ctx.stroke(
                Path { p in
                    p.move(to: CGPoint(x: rect.minX + IconMetrics.margin, y: rect.maxY - 5))
                    p.addLine(to: CGPoint(x: rect.maxX - IconMetrics.margin, y: rect.minY + 5))
                },
                with: .color(color),
                style: StrokeStyle(lineWidth: IconMetrics.stroke, lineCap: .round)
            )
        }
    }

    static func ellipse(color: Color) -> some View {
        IconCanvas(color: color) { ctx, rect in
            ctx.stroke(
                Path(ellipseIn: rect.insetBy(dx: IconMetrics.margin, dy: 5)),
                with: .color(color),
                style: StrokeStyle(lineWidth: IconMetrics.stroke)
            )
        }
    }

    static func arrow(color: Color) -> some View {
        IconCanvas(color: color) { ctx, rect in
            let start = CGPoint(x: rect.minX + IconMetrics.margin, y: rect.maxY - 5)
            let tip = CGPoint(x: rect.maxX - IconMetrics.margin, y: rect.minY + 5)
            let angle = atan2(tip.y - start.y, tip.x - start.x)
            let barbAngle: CGFloat = 2.5
            let barbLength: CGFloat = 6
            let left = CGPoint(
                x: tip.x + barbLength * cos(angle + barbAngle),
                y: tip.y + barbLength * sin(angle + barbAngle)
            )
            let right = CGPoint(
                x: tip.x + barbLength * cos(angle - barbAngle),
                y: tip.y + barbLength * sin(angle - barbAngle)
            )
            ctx.stroke(
                Path { p in
                    p.move(to: start)
                    p.addLine(to: tip)
                    p.move(to: tip)
                    p.addLine(to: left)
                    p.move(to: tip)
                    p.addLine(to: right)
                },
                with: .color(color),
                style: StrokeStyle(lineWidth: IconMetrics.stroke, lineCap: .round, lineJoin: .round)
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
                style: StrokeStyle(lineWidth: IconMetrics.stroke, lineCap: .round)
            )
        }
    }

    static func eye(color: Color, open: Bool = true) -> some View {
        IconCanvas(color: color) { ctx, rect in
            let lid = Path(ellipseIn: rect.insetBy(dx: 3, dy: 6))
            ctx.stroke(lid, with: .color(color), style: StrokeStyle(lineWidth: IconMetrics.stroke))
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
                        p.move(to: CGPoint(x: rect.minX + IconMetrics.margin, y: rect.maxY - 5))
                        p.addLine(to: CGPoint(x: rect.maxX - IconMetrics.margin, y: rect.minY + 5))
                    },
                    with: .color(color),
                    style: StrokeStyle(lineWidth: IconMetrics.stroke, lineCap: .round)
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
                style: StrokeStyle(lineWidth: IconMetrics.stroke, lineCap: .round, lineJoin: .round)
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
