import SwiftUI

enum CalmTooltipEdge {
    case leading
    case trailing
}

struct CalmTooltipModifier: ViewModifier {
    @Environment(\.themeColors) private var colors
    let text: String
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
            .accessibilityLabel(text)
    }

    private var tip: some View {
        Text(text)
            .font(.system(size: Tokens.TypeSize.label, weight: .medium))
            .foregroundStyle(colors.text)
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
    func calmTooltip(_ text: String, edge: CalmTooltipEdge = .trailing) -> some View {
        modifier(CalmTooltipModifier(text: text, edge: edge))
    }
}
