import SwiftUI

enum ToastKind {
    case success
    case error
}

/// A one-shot status message — `Engine` is the only thing that creates these (an AI op
/// finishing, say), and only ever holds one at a time. `id` exists so a dismiss timer can
/// tell "the toast I scheduled" apart from "a newer toast that replaced it" without a race.
struct ToastMessage: Identifiable, Equatable {
    let id = UUID()
    let text: String
    let kind: ToastKind
}

/// A transient status banner, shown for a few seconds and then gone on its own — for
/// operations too short-lived to deserve a persistent home in the UI (an AI op's result,
/// for instance) but too important to happen silently.
struct CalmToastView: View {
    @Environment(\.themeColors) private var colors
    let toast: ToastMessage

    var body: some View {
        HStack(spacing: Tokens.Space.sm) {
            Image(systemName: toast.kind == .success ? "checkmark.circle.fill" : "exclamationmark.triangle.fill")
                .foregroundStyle(accent)
            Text(toast.text)
                .font(.system(size: Tokens.TypeSize.body, weight: .medium))
                .foregroundStyle(colors.text)
        }
        .padding(.horizontal, Tokens.Space.md)
        .padding(.vertical, Tokens.Space.sm)
        .background(
            colors.surfaceHover,
            in: RoundedRectangle(cornerRadius: Tokens.Radius.md, style: .continuous)
        )
        .overlay(
            RoundedRectangle(cornerRadius: Tokens.Radius.md, style: .continuous)
                .strokeBorder(accent.opacity(0.5), lineWidth: 1)
        )
        .shadow(color: .black.opacity(0.2), radius: 12, y: 4)
        .fixedSize()
    }

    private var accent: Color {
        toast.kind == .success ? colors.accentTeal : colors.danger
    }
}

extension View {
    /// Shows `toast` (from `Engine`) at the top of this view, sliding in and fading out on
    /// its own — the caller only has to keep passing the current value through, nothing else.
    func calmToast(_ toast: ToastMessage?) -> some View {
        overlay(alignment: .top) {
            if let toast {
                CalmToastView(toast: toast)
                    .padding(.top, Tokens.Space.lg)
                    .transition(.move(edge: .top).combined(with: .opacity))
                    .allowsHitTesting(false)
                    .zIndex(200)
            }
        }
        .animation(.spring(response: 0.35, dampingFraction: 0.8), value: toast)
    }
}
