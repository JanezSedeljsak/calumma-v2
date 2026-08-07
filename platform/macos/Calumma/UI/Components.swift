import SwiftUI

enum CalmText {
    static func brand(_ text: String) -> some View {
        BrandText(text: text)
    }

    static func eyebrow(_ text: String) -> some View {
        EyebrowText(text: text)
    }

    static func label(_ text: String) -> some View {
        LabelText(text: text)
    }

    static func title(_ text: String, strong: Bool = false) -> some View {
        TitleText(text: text, strong: strong)
    }

    static func body(_ text: String, strong: Bool = false) -> some View {
        BodyText(text: text, strong: strong)
    }

    static func muted(_ text: String, mono: Bool = false) -> some View {
        MutedText(text: text, mono: mono)
    }
}

private struct BrandText: View {
    @Environment(\.themeColors) private var colors
    let text: String

    var body: some View {
        Text(text)
            .font(.system(size: Tokens.TypeSize.brand, weight: .bold))
            .foregroundStyle(
                LinearGradient(
                    colors: [colors.accentTeal, colors.accentOrange],
                    startPoint: .leading,
                    endPoint: .trailing
                )
            )
    }
}

private struct EyebrowText: View {
    @Environment(\.themeColors) private var colors
    let text: String

    var body: some View {
        Text(text.uppercased())
            .font(.system(size: Tokens.TypeSize.label, weight: .semibold))
            .tracking(1.2)
            .foregroundStyle(colors.textMuted)
    }
}

private struct LabelText: View {
    @Environment(\.themeColors) private var colors
    let text: String

    var body: some View {
        Text(text.uppercased())
            .font(.system(size: Tokens.TypeSize.label, weight: .semibold))
            .tracking(Tokens.TypeSize.labelTracking * 10)
            .foregroundStyle(colors.textMuted)
    }
}

private struct TitleText: View {
    @Environment(\.themeColors) private var colors
    let text: String
    var strong = false

    var body: some View {
        Text(text)
            .font(.system(size: Tokens.TypeSize.title, weight: strong ? .bold : .semibold))
            .foregroundStyle(colors.text)
    }
}

private struct BodyText: View {
    @Environment(\.themeColors) private var colors
    let text: String
    var strong = false

    var body: some View {
        Text(text)
            .font(.system(size: Tokens.TypeSize.body, weight: strong ? .semibold : .regular))
            .foregroundStyle(colors.text)
    }
}

private struct MutedText: View {
    @Environment(\.themeColors) private var colors
    let text: String
    var mono = false

    var body: some View {
        Text(text)
            .font(mono
                ? .system(size: Tokens.TypeSize.label, weight: .medium).monospacedDigit()
                : .system(size: Tokens.TypeSize.label))
            .foregroundStyle(colors.textMuted)
    }
}

struct CalmIsland<Content: View>: View {
    @Environment(\.themeColors) private var colors
    var padding: CGFloat = Tokens.Space.md
    @ViewBuilder let content: () -> Content

    var body: some View {
        content()
            .padding(padding)
            .background(
                colors.surface,
                in: RoundedRectangle(cornerRadius: Tokens.Radius.sm, style: .continuous)
            )
    }
}

struct CalmField: View {
    @Environment(\.themeColors) private var colors
    @Binding var text: String

    var body: some View {
        TextField("", text: $text)
            .textFieldStyle(.plain)
            .padding(.horizontal, Tokens.Space.md)
            .padding(.vertical, Tokens.Space.md)
            .calmSurface()
            .foregroundStyle(colors.text)
    }
}

struct CalmNumberField: View {
    @Environment(\.themeColors) private var colors
    @Binding var value: Int
    var width: CGFloat = 88

    var body: some View {
        TextField("", value: $value, format: .number)
            .textFieldStyle(.plain)
            .frame(width: width)
            .padding(.horizontal, Tokens.Space.md)
            .padding(.vertical, Tokens.Space.md)
            .calmSurface()
            .foregroundStyle(colors.text)
    }
}

struct CalmAccentButton: View {
    @Environment(\.themeColors) private var colors
    let title: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Text(title)
                .font(.system(size: Tokens.TypeSize.body, weight: .bold))
                .foregroundStyle(colors.accentTeal)
                .padding(.horizontal, Tokens.Space.lg)
                .padding(.vertical, Tokens.Space.md)
                .calmSurface()
        }
        .buttonStyle(.plain)
        .calmPointer()
    }
}

struct CalmPlainButton: View {
    @Environment(\.themeColors) private var colors
    let title: String
    var enabled = true
    var accent = false
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Text(title)
                .foregroundStyle(
                    accent ? colors.accentTeal : (enabled ? colors.text : colors.textMuted)
                )
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
        .calmPointer()
    }
}

struct CalmIconButton<Icon: View>: View {
    let action: () -> Void
    @ViewBuilder let icon: () -> Icon

    var body: some View {
        Button(action: action) {
            icon()
                .padding(Tokens.Space.sm)
                .calmSurface()
        }
        .buttonStyle(.plain)
        .calmPointer()
    }
}

struct CalmToolButton<Icon: View>: View {
    let selected: Bool
    let action: () -> Void
    let icon: Icon

    init(selected: Bool, action: @escaping () -> Void, @ViewBuilder icon: () -> Icon) {
        self.selected = selected
        self.action = action
        self.icon = icon()
    }

    var body: some View {
        Button(action: action) {
            icon
                .padding(Tokens.Space.sm)
                .frame(width: 36, height: 36)
                .calmSurface(hover: selected, radius: Tokens.Radius.sm)
        }
        .buttonStyle(.plain)
        .calmPointer()
    }
}

struct CalmSection<Content: View>: View {
    let title: String
    var accent: Color? = nil
    @ViewBuilder let content: () -> Content

    var body: some View {
        VStack(alignment: .leading, spacing: Tokens.Space.md) {
            HStack(spacing: Tokens.Space.sm) {
                if let accent {
                    Circle().fill(accent).frame(width: 8, height: 8)
                }
                CalmText.label(title)
            }
            content()
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

struct CalmRowButton<Content: View>: View {
    let action: () -> Void
    @ViewBuilder let content: () -> Content

    var body: some View {
        Button(action: action) {
            content()
                .padding(Tokens.Space.md)
                .frame(maxWidth: .infinity, alignment: .leading)
                .calmSurface()
        }
        .buttonStyle(.plain)
        .calmPointer()
    }
}

struct CalmRow<Leading: View>: View {
    @ViewBuilder let leading: () -> Leading
    let title: String
    let subtitle: String
    var trailing: String? = nil
    var useTitleSize = false

    var body: some View {
        HStack(spacing: Tokens.Space.md) {
            leading()
            VStack(alignment: .leading, spacing: 2) {
                if useTitleSize {
                    CalmText.title(title)
                } else {
                    CalmText.body(title, strong: true)
                }
                CalmText.muted(subtitle)
            }
            Spacer()
            if let trailing {
                CalmText.muted(trailing)
            }
        }
    }
}

struct CalmThumb: View {
    @Environment(\.themeColors) private var colors
    var tint: Color? = nil
    var width: CGFloat = 36
    var height: CGFloat = 36
    var label: String? = nil

    var body: some View {
        RoundedRectangle(cornerRadius: Tokens.Radius.sm, style: .continuous)
            .fill(tint ?? colors.surfaceHover)
            .frame(width: width, height: height)
            .overlay {
                if let label {
                    Text(label)
                        .font(.system(size: Tokens.TypeSize.label, weight: .bold))
                        .foregroundStyle(colors.textMuted)
                }
            }
    }
}

struct CalmChip: View {
    @Environment(\.themeColors) private var colors
    let title: String
    let selected: Bool
    let onSelect: () -> Void
    let onClose: () -> Void

    var body: some View {
        HStack(spacing: Tokens.Space.sm) {
            Button(action: onSelect) {
                Text(title)
                    .font(.system(size: Tokens.TypeSize.body, weight: selected ? .bold : .medium))
                    .foregroundStyle(selected ? colors.text : colors.textMuted)
            }
            .buttonStyle(.plain)
            Button(action: onClose) {
                Text("×").foregroundStyle(colors.textMuted)
            }
            .buttonStyle(.plain)
        }
        .padding(.horizontal, Tokens.Space.md)
        .padding(.vertical, Tokens.Space.sm)
        .background(
            selected ? colors.surface : colors.surface.opacity(0.001),
            in: RoundedRectangle(cornerRadius: Tokens.Radius.sm, style: .continuous)
        )
        .calmPointer()
    }
}
