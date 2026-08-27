import SwiftUI

/// What the canvas shows while a project is being read back out of SQLite: the desk, and one
/// sweeping rectangle exactly where the paper is about to land. It is not board content — the
/// Metal view underneath is still holding the project you are leaving — so this covers the
/// board rather than drawing on it, which is the difference between standing in for the canvas
/// and styling it (`docs/STYLE.md` rule 7).
///
/// The rectangle comes from `Engine.fitSize`, the engine's own fit geometry, so the placeholder
/// and the paper that replaces it occupy the same points and the switch has nothing to jump.
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
