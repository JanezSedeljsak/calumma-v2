import AppKit
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
                in: RoundedRectangle(cornerRadius: Tokens.Radius.island, style: .continuous)
            )
            .overlay(
                RoundedRectangle(cornerRadius: Tokens.Radius.island, style: .continuous)
                    .strokeBorder(colors.islandBorder, lineWidth: 1)
            )
    }
}

/// Inputs and buttons sit on `Tokens.Control.height` rather than the spacing scale: a
/// control's height is a control metric, and every field *and button* in the app moves
/// together when it changes — a Create button next to a resolution field is one row, and
/// two controls in a row that disagree by a few points read as a mistake. Horizontal
/// padding stays on the spacing scale.
struct CalmField: View {
    @Environment(\.themeColors) private var colors
    @Binding var text: String
    @FocusState private var focused: Bool

    var body: some View {
        TextField("", text: $text)
            .textFieldStyle(.plain)
            .focused($focused)
            .padding(.horizontal, Tokens.Space.md)
            .frame(height: Tokens.Control.height)
            .calmSurface(bordered: true, focused: focused)
            .foregroundStyle(colors.text)
    }
}

struct CalmNumberField: View {
    @Environment(\.themeColors) private var colors
    @Binding var value: Int
    var width: CGFloat = 88
    @FocusState private var focused: Bool

    var body: some View {
        TextField("", value: $value, format: .number)
            .textFieldStyle(.plain)
            .focused($focused)
            .frame(width: width)
            .padding(.horizontal, Tokens.Space.md)
            .frame(height: Tokens.Control.height)
            .calmSurface(bordered: true, focused: focused)
            .foregroundStyle(colors.text)
    }
}

/// The number beside a slider, typed rather than dragged. A range that runs to 1000 cannot
/// be dialled to an exact 137 on a 96pt-wide slider whatever curve it is on, so every size
/// slider carries one of these. Deliberately *not* on `Tokens.Control.height`: it lives in
/// the tools panel's denser scale, next to label-size type, where a form-height field would
/// tower over the row it belongs to.
///
/// The text is local while it is being typed — committing on every keystroke would clamp
/// "10" out from under someone on their way to "100" — and is written back from the value on
/// submit, on blur, and whenever the slider moves it.
struct CalmSliderValueField: View {
    @Environment(\.themeColors) private var colors
    @Binding var value: Float
    let range: ClosedRange<Float>
    var width: CGFloat = 38

    @State private var text = ""
    @FocusState private var focused: Bool

    var body: some View {
        TextField("", text: $text)
            .textFieldStyle(.plain)
            .font(.system(size: Tokens.TypeSize.label, weight: .medium).monospacedDigit())
            .multilineTextAlignment(.trailing)
            .focused($focused)
            .foregroundStyle(colors.text)
            .frame(width: width)
            .padding(.horizontal, Tokens.Space.xs)
            .padding(.vertical, 2)
            .calmSurface(radius: Tokens.Radius.sm, bordered: true, focused: focused)
            .onSubmit { commit() }
            .onChange(of: focused) { _, isFocused in
                if !isFocused { commit() }
            }
            .onChange(of: value) { _, next in
                if !focused { text = Self.format(next) }
            }
            .onAppear { text = Self.format(value) }
    }

    private func commit() {
        if let typed = Float(text.trimmingCharacters(in: .whitespaces)) {
            value = min(max(typed, range.lowerBound), range.upperBound)
        }
        text = Self.format(value)
    }

    private static func format(_ value: Float) -> String {
        "\(Int(value.rounded()))"
    }
}

/// Buttons are filled, bordered controls one input tall: the border is what tells you
/// where the hit target ends, and on a card whose background is already `surface` it is
/// the *only* thing that does. Hover shifts luminance (`docs/STYLE.md` rule 5).
private struct CalmButtonSurface: ViewModifier {
    @State private var hovering = false
    var padX: CGFloat = Tokens.Space.md
    var enabled = true
    /// Buttons laid out in a grid or a column fill their slot so the edges line up;
    /// a button sitting next to a field keeps its intrinsic width.
    var fill = false

    func body(content: Content) -> some View {
        content
            .lineLimit(1)
            .minimumScaleFactor(0.85)
            .padding(.horizontal, padX)
            .frame(maxWidth: fill ? .infinity : nil)
            .frame(height: Tokens.Control.height)
            .calmSurface(hover: hovering && enabled, bordered: true)
            .contentShape(Rectangle())
            .onHover { hovering = $0 }
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
                .modifier(CalmButtonSurface(padX: Tokens.Space.lg))
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
    var fill = false
    /// An explicit color for the few buttons that are neither ordinary nor the accent —
    /// today just Delete, which is `color.danger` per `docs/STYLE.md`'s hierarchy table.
    var tint: Color?
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Text(title)
                .foregroundStyle(
                    tint ?? (accent ? colors.accentTeal : (enabled ? colors.text : colors.textMuted))
                )
                .modifier(CalmButtonSurface(enabled: enabled, fill: fill))
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
        .calmPointer()
    }
}

enum CalmToolButtonLayout {
    static let size: CGFloat = 32
    /// How far a tool the layer cannot take drops out of the grid. Enough to read as off at a
    /// glance, not so far that the icon stops being identifiable.
    static let disabledOpacity: CGFloat = 0.35
}

struct CalmToolButton<Icon: View>: View {
    let selected: Bool
    let action: () -> Void
    var tooltip: String? = nil
    var tooltipEdge: CalmTooltipEdge = .trailing
    /// A tool the active layer cannot take is off, not hidden — the grid keeps its shape, and
    /// the tooltip carries the reason. A luminance drop, per `docs/STYLE.md`: no badge, no
    /// second colour, nothing red.
    var enabled: Bool = true
    let icon: Icon

    init(
        selected: Bool,
        action: @escaping () -> Void,
        tooltip: String? = nil,
        tooltipEdge: CalmTooltipEdge = .trailing,
        enabled: Bool = true,
        @ViewBuilder icon: () -> Icon
    ) {
        self.selected = selected
        self.action = action
        self.tooltip = tooltip
        self.tooltipEdge = tooltipEdge
        self.enabled = enabled
        self.icon = icon()
    }

    var body: some View {
        Button(action: action) {
            icon
                .padding(Tokens.Space.xs)
                .frame(width: CalmToolButtonLayout.size, height: CalmToolButtonLayout.size)
                .calmSurface(hover: selected, radius: Tokens.Radius.sm)
                .opacity(enabled ? 1 : CalmToolButtonLayout.disabledOpacity)
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
        .calmPointer(enabled)
        .modifier(OptionalCalmTooltip(text: tooltip, edge: tooltipEdge))
    }
}

private struct OptionalCalmTooltip: ViewModifier {
    let text: String?
    var edge: CalmTooltipEdge = .trailing

    func body(content: Content) -> some View {
        if let text {
            content.calmTooltip(text, edge: edge)
        } else {
            content
        }
    }
}

struct CalmDivider: View {
    @Environment(\.themeColors) private var colors

    var body: some View {
        Rectangle()
            .fill(colors.islandBorder)
            .frame(height: 1)
            .frame(maxWidth: .infinity)
    }
}

struct CalmSection<Content: View, Trailing: View>: View {
    let title: String
    var accent: Color? = nil
    /// The gap between the section's own rows, separately from the gap under its title — a
    /// list of cards wants tighter, more even spacing than the header does.
    var contentSpacing: CGFloat = Tokens.Space.md
    @ViewBuilder let trailing: () -> Trailing
    @ViewBuilder let content: () -> Content

    init(
        title: String,
        accent: Color? = nil,
        contentSpacing: CGFloat = Tokens.Space.md,
        @ViewBuilder content: @escaping () -> Content
    ) where Trailing == EmptyView {
        self.title = title
        self.accent = accent
        self.contentSpacing = contentSpacing
        self.trailing = { EmptyView() }
        self.content = content
    }

    init(
        title: String,
        accent: Color? = nil,
        contentSpacing: CGFloat = Tokens.Space.md,
        @ViewBuilder trailing: @escaping () -> Trailing,
        @ViewBuilder content: @escaping () -> Content
    ) {
        self.title = title
        self.accent = accent
        self.contentSpacing = contentSpacing
        self.trailing = trailing
        self.content = content
    }

    var body: some View {
        VStack(alignment: .leading, spacing: Tokens.Space.md) {
            HStack(spacing: Tokens.Space.sm) {
                HStack(spacing: Tokens.Space.sm) {
                    if let accent {
                        Circle().fill(accent).frame(width: 8, height: 8)
                    }
                    CalmText.label(title)
                }
                Spacer(minLength: Tokens.Space.sm)
                trailing()
            }
            VStack(alignment: .leading, spacing: contentSpacing) {
                content()
            }
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
                .padding(.horizontal, Tokens.Space.md)
                .padding(.vertical, Tokens.Space.sm)
                .frame(maxWidth: .infinity, alignment: .leading)
                .calmSurface(bordered: true)
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
                Group {
                    if useTitleSize {
                        CalmText.title(title)
                    } else {
                        CalmText.body(title, strong: true)
                    }
                }
                .lineLimit(1)
                .truncationMode(.tail)
                .help(title)
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
    var image: NSImage? = nil
    var width: CGFloat = 36
    var height: CGFloat = 36
    var label: String? = nil

    var body: some View {
        RoundedRectangle(cornerRadius: Tokens.Radius.sm, style: .continuous)
            .fill(tint ?? colors.surfaceHover)
            .frame(width: width, height: height)
            .overlay {
                if let image {
                    Image(nsImage: image)
                        .resizable()
                        .scaledToFill()
                        .clipShape(
                            RoundedRectangle(cornerRadius: Tokens.Radius.sm, style: .continuous)
                        )
                } else if let label {
                    Text(label)
                        .font(.system(size: Tokens.TypeSize.label, weight: .bold))
                        .foregroundStyle(colors.textMuted)
                }
            }
            .clipped()
    }
}

struct ProjectThumbView: View {
    @EnvironmentObject private var app: AppModel
    let projectId: String
    var tint: Color
    var width: CGFloat = 72
    var height: CGFloat = 72
    @State private var image: NSImage?

    var body: some View {
        CalmThumb(tint: tint, image: image, width: width, height: height)
            .task(id: "\(projectId)-\(app.engine.thumbnailRevision)") {
                guard let data = app.engine.projectThumbnailPNG(projectId: projectId) else {
                    image = nil
                    return
                }
                image = NSImage(data: data)
            }
    }
}

struct CalmDot: View {
    let color: Color
    var size: CGFloat = 9
    var selected = false

    var body: some View {
        Circle()
            .fill(color)
            .frame(width: size, height: size)
            .padding(selected ? 3 : 0)
            .background(
                selected ? color.opacity(0.35) : color.opacity(0),
                in: Circle()
            )
    }
}

struct CalmPaletteRow: View {
    let colors: [Color]
    let selected: Color
    let onPick: (Color) -> Void

    var body: some View {
        HStack(spacing: Tokens.Space.sm) {
            ForEach(Array(colors.enumerated()), id: \.offset) { _, color in
                Button {
                    onPick(color)
                } label: {
                    CalmDot(
                        color: color,
                        size: 14,
                        selected: color.packedRGB == selected.packedRGB
                    )
                }
                .buttonStyle(.plain)
                .calmPointer()
            }
        }
    }
}

/// A modal that behaves the way people expect one to: a dimmed scrim you can click to leave,
/// and Esc.
///
/// This is an overlay rather than a `.sheet` because a macOS sheet offers neither — it blocks
/// the parent window, so there is no "outside" left to click. The scrim is the affordance; a
/// floating `×` pinned over the card's corner belongs to whatever the card is, not to the
/// wrapper, so a view that wants one puts it in its own header where it can be laid out.
struct CalmModal<Modal: View>: ViewModifier {
    @Binding var isPresented: Bool
    @ViewBuilder let modal: () -> Modal

    func body(content: Content) -> some View {
        content.overlay {
            if isPresented {
                ZStack {
                    Rectangle()
                        .fill(Color.black.opacity(0.35))
                        .ignoresSafeArea()
                        .contentShape(Rectangle())
                        .onTapGesture { isPresented = false }

                    modal()
                        .clipShape(
                            RoundedRectangle(cornerRadius: Tokens.Radius.window, style: .continuous)
                        )
                        .shadow(color: .black.opacity(0.3), radius: 24, y: 8)
                        .onExitCommand { isPresented = false }
                }
                .transition(.opacity)
                .zIndex(500)
            }
        }
        .animation(.easeOut(duration: 0.15), value: isPresented)
    }
}

extension View {
    func calmModal<Modal: View>(
        isPresented: Binding<Bool>,
        @ViewBuilder modal: @escaping () -> Modal
    ) -> some View {
        modifier(CalmModal(isPresented: isPresented, modal: modal))
    }
}
