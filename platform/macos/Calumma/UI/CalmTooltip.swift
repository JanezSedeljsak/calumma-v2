import SwiftUI

enum CalmTooltipEdge {
    case leading
    case trailing
}

struct CalmTooltipModifier: ViewModifier {
    @Environment(\.themeColors) private var colors
    let text: String
    /// The key that does the same thing, printed beside the name. Nothing is shown for a
    /// control with no shortcut — and nothing beside a *refusal*, where the tooltip carries a
    /// reason rather than a name and a key would only look like a way around it.
    var shortcut: String? = nil
    var edge: CalmTooltipEdge = .trailing
    var delay: Duration = .milliseconds(450)

    @State private var hovering = false
    @State private var visible = false
    @State private var showTask: Task<Void, Never>?

    func body(content: Content) -> some View {
        content
            .onHover { inside in
                hovering = inside
                showTask?.cancel()
                if inside {
                    showTask = Task { @MainActor in
                        try? await Task.sleep(for: delay)
                        guard !Task.isCancelled, hovering else { return }
                        visible = true
                    }
                } else {
                    visible = false
                }
            }
            .overlay(alignment: edge == .trailing ? .trailing : .leading) {
                if visible {
                    tip
                        .offset(x: edge == .trailing ? Tokens.Space.sm : -Tokens.Space.sm)
                        .alignmentGuide(edge == .trailing ? .trailing : .leading) { dim in
                            edge == .trailing ? dim[.leading] : dim[.trailing]
                        }
                        .transition(.opacity)
                        .allowsHitTesting(false)
                        .zIndex(100)
                }
            }
            .animation(.easeOut(duration: 0.12), value: visible)
            .accessibilityLabel(shortcut.map { "\(text) (\($0))" } ?? text)
    }

    private var tip: some View {
        HStack(spacing: Tokens.Space.sm) {
            Text(text)
                .font(.system(size: Tokens.TypeSize.label, weight: .medium))
                .foregroundStyle(colors.text)
            if let shortcut {
                // Muted mono, not a key cap: a bordered chip at this scale reads as dirt, and
                // rule 1 in `docs/STYLE.md` keeps borders off chips for exactly that reason.
                Text(shortcut)
                    .font(.system(size: Tokens.TypeSize.label, weight: .semibold, design: .monospaced))
                    .foregroundStyle(colors.textMuted)
            }
        }
        .padding(.horizontal, Tokens.Space.sm)
        .padding(.vertical, Tokens.Space.xs)
        .background(
            colors.surfaceHover,
            in: RoundedRectangle(cornerRadius: Tokens.Radius.sm, style: .continuous)
        )
        .overlay(
            RoundedRectangle(cornerRadius: Tokens.Radius.sm, style: .continuous)
                .strokeBorder(colors.islandBorder, lineWidth: 1)
        )
        .fixedSize()
    }
}

extension View {
    func calmTooltip(
        _ text: String,
        shortcut: String? = nil,
        edge: CalmTooltipEdge = .trailing
    ) -> some View {
        modifier(CalmTooltipModifier(text: text, shortcut: shortcut, edge: edge))
    }
}
