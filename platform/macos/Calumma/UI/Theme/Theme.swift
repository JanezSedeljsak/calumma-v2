import AppKit
import SwiftUI

enum AppTheme: String, CaseIterable {
    case light
    case dark

    var isDark: Bool { self == .dark }
}

struct ThemeColors {
    let bg: Color
    let surface: Color
    let surfaceHover: Color
    let text: Color
    let textMuted: Color
    let accentTeal: Color
    let accentOrange: Color
    let danger: Color
    let desk: Color
    let deskGrid: Color
    let paperBorder: Color
    let islandBorder: Color
    let controlBorder: Color
    let controlFocusBorder: Color

    static func colors(for theme: AppTheme) -> ThemeColors {
        switch theme {
        case .light:
            return ThemeColors(
                bg: Tokens.Light.bg,
                surface: Tokens.Light.surface,
                surfaceHover: Tokens.Light.surfaceHover,
                text: Tokens.Light.text,
                textMuted: Tokens.Light.textMuted,
                accentTeal: Tokens.Light.accentTeal,
                accentOrange: Tokens.Light.accentOrange,
                danger: Tokens.Light.danger,
                desk: Tokens.Light.desk,
                deskGrid: Tokens.Light.deskGrid,
                paperBorder: Tokens.Light.paperBorder,
                islandBorder: Tokens.Light.islandBorder,
                controlBorder: Tokens.Light.controlBorder,
                controlFocusBorder: Tokens.Light.controlFocusBorder
            )
        case .dark:
            return ThemeColors(
                bg: Tokens.Dark.bg,
                surface: Tokens.Dark.surface,
                surfaceHover: Tokens.Dark.surfaceHover,
                text: Tokens.Dark.text,
                textMuted: Tokens.Dark.textMuted,
                accentTeal: Tokens.Dark.accentTeal,
                accentOrange: Tokens.Dark.accentOrange,
                danger: Tokens.Dark.danger,
                desk: Tokens.Dark.desk,
                deskGrid: Tokens.Dark.deskGrid,
                paperBorder: Tokens.Dark.paperBorder,
                islandBorder: Tokens.Dark.islandBorder,
                controlBorder: Tokens.Dark.controlBorder,
                controlFocusBorder: Tokens.Dark.controlFocusBorder
            )
        }
    }
}

private struct ThemeColorsKey: EnvironmentKey {
    static let defaultValue = ThemeColors.colors(for: .dark)
}

extension EnvironmentValues {
    var themeColors: ThemeColors {
        get { self[ThemeColorsKey.self] }
        set { self[ThemeColorsKey.self] = newValue }
    }
}

extension View {
    func themeColors(_ colors: ThemeColors) -> some View {
        environment(\.themeColors, colors)
    }

    func calmScreen() -> some View {
        modifier(CalmScreenBackground())
    }

    func calmPanel() -> some View {
        modifier(CalmPanelBackground())
    }

    func calmSurface(
        hover: Bool = false,
        radius: CGFloat = Tokens.Radius.md,
        bordered: Bool = false,
        focused: Bool = false
    ) -> some View {
        modifier(
            CalmSurfaceBackground(
                hover: hover,
                radius: radius,
                bordered: bordered,
                focused: focused
            )
        )
    }

    /// `active: false` for a control that is switched off — a hand cursor over something
    /// that will not respond is the same false promise a lit-up icon would be.
    func calmPointer(_ active: Bool = true) -> some View {
        modifier(CalmPointerCursor(active: active))
    }

    /// Every scrollable area in the app floats its scroll indicator over the content and
    /// fades it out at rest, regardless of the user's system-wide scroll bar preference —
    /// place inside a `ScrollView`, on the content it wraps.
    func calmScrollBars() -> some View {
        background(CalmScrollBarConfigurator())
    }
}

private struct CalmPointerCursor: ViewModifier {
    var active = true

    func body(content: Content) -> some View {
        content.onHover { hovering in
            guard active else { return }
            if hovering {
                NSCursor.pointingHand.push()
            } else {
                NSCursor.pop()
            }
        }
    }
}

private struct CalmScrollBarConfigurator: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        DispatchQueue.main.async { configure(view) }
        return view
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        configure(nsView)
    }

    private func configure(_ view: NSView) {
        guard let scrollView = view.enclosingScrollView else { return }
        scrollView.scrollerStyle = .overlay
        scrollView.autohidesScrollers = true
    }
}

private struct CalmScreenBackground: ViewModifier {
    @Environment(\.themeColors) private var colors

    func body(content: Content) -> some View {
        content.background(colors.bg)
    }
}

private struct CalmPanelBackground: ViewModifier {
    @Environment(\.themeColors) private var colors

    func body(content: Content) -> some View {
        content.background(colors.surface)
    }
}

private struct CalmSurfaceBackground: ViewModifier {
    @Environment(\.themeColors) private var colors
    var hover = false
    var radius: CGFloat = Tokens.Radius.md
    var bordered = false
    var focused = false

    func body(content: Content) -> some View {
        let shape = RoundedRectangle(cornerRadius: radius, style: .continuous)
        return content
            .background(hover ? colors.surfaceHover : colors.surface, in: shape)
            .overlay {
                if bordered {
                    shape.strokeBorder(
                        focused ? colors.controlFocusBorder : colors.controlBorder,
                        lineWidth: 1
                    )
                }
            }
    }
}
