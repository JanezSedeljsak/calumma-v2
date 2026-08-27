import SwiftUI

/// What the canvas shows while a project is being read back out of SQLite: the desk — squared
/// paper and all — and one sweeping rectangle exactly where the paper is about to land. It is
/// not board content, the Metal view underneath is still holding the project you are leaving,
/// so this covers the board rather than drawing on it, which is the difference between standing
/// in for the canvas and styling it (`docs/STYLE.md` rule 7).
///
/// Both halves are the engine's geometry, not a lookalike: the rectangle comes from
/// `Engine.fitSize` and the grid from `Engine.desk`, the same table `board.wgsl` lays the real
/// desk on. Nothing shifts when the real board arrives.
struct CanvasSkeleton: View {
    @Environment(\.themeColors) private var colors
    let document: CGSize

    @State private var phase: CGFloat = 0

    private static let sweepSeconds: Double = 1.1
    /// The band's width as a fraction of the paper's — wide enough to read as a sweep rather
    /// than a line crossing the board.
    private static let bandFraction: CGFloat = 0.4

    var body: some View {
        GeometryReader { proxy in
            let paper = Engine.fitSize(viewport: proxy.size, document: document)
            ZStack {
                colors.desk
                desk
                Rectangle()
                    .fill(colors.paper)
                    .overlay { sweep }
                    .overlay { Rectangle().strokeBorder(colors.paperBorder, lineWidth: 1) }
                    .frame(width: paper.width, height: paper.height)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .allowsHitTesting(false)
        .onAppear {
            withAnimation(.linear(duration: Self.sweepSeconds).repeatForever(autoreverses: false)) {
                phase = 1
            }
        }
    }

    /// The squared paper behind the board: a faint rule down each cell edge and a cross on every
    /// corner, exactly as `desk_pattern` draws it. Anchored at this view's origin, which is the
    /// board viewport's origin — the same place the shader measures its screen coordinates from,
    /// so the two lattices line up rather than merely matching in spacing.
    private var desk: some View {
        Canvas { context, size in
            let metrics = Engine.desk
            let rules = Path { path in
                var x: CGFloat = 0
                while x < size.width {
                    path.addRect(CGRect(x: x, y: 0, width: metrics.lineWidth, height: size.height))
                    x += metrics.cell
                }
                var y: CGFloat = 0
                while y < size.height {
                    path.addRect(CGRect(x: 0, y: y, width: size.width, height: metrics.lineWidth))
                    y += metrics.cell
                }
            }
            context.fill(rules, with: .color(colors.deskGrid.opacity(metrics.lineAlpha)))

            let arm = metrics.crossArm
            let thickness = metrics.crossLineWidth
            let crosses = Path { path in
                var y: CGFloat = 0
                while y <= size.height + arm {
                    var x: CGFloat = 0
                    while x <= size.width + arm {
                        path.addRect(
                            CGRect(x: x - thickness / 2, y: y - arm, width: thickness, height: arm * 2)
                        )
                        path.addRect(
                            CGRect(x: x - arm, y: y - thickness / 2, width: arm * 2, height: thickness)
                        )
                        x += metrics.cell
                    }
                    y += metrics.cell
                }
            }
            context.fill(crosses, with: .color(colors.deskGrid))
        }
    }

    /// A band of the raised surface color travelling across the paper: a luminance shift and
    /// nothing else, per `docs/STYLE.md` — no second color, no outline, nothing that could be
    /// mistaken for something already drawn on the board.
    private var sweep: some View {
        GeometryReader { proxy in
            let band = max(proxy.size.width * Self.bandFraction, 1)
            LinearGradient(
                colors: [.clear, colors.surfaceHover, .clear],
                startPoint: .leading,
                endPoint: .trailing
            )
            .frame(width: band)
            .offset(x: phase * (proxy.size.width + band) - band)
        }
        .clipped()
    }
}
