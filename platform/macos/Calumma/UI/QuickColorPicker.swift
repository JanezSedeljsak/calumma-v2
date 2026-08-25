import AppKit
import SwiftUI

struct HSBColor: Equatable {
    var hue: Double
    var saturation: Double
    var brightness: Double

    init(hue: Double, saturation: Double, brightness: Double) {
        self.hue = min(max(hue, 0), 1)
        self.saturation = min(max(saturation, 0), 1)
        self.brightness = min(max(brightness, 0), 1)
    }

    init(_ color: Color) {
        let ns = NSColor(color).usingColorSpace(.sRGB) ?? .black
        var h: CGFloat = 0
        var s: CGFloat = 0
        var b: CGFloat = 0
        ns.getHue(&h, saturation: &s, brightness: &b, alpha: nil)
        self.init(hue: Double(h), saturation: Double(s), brightness: Double(b))
    }

    var color: Color {
        Color(hue: hue, saturation: saturation, brightness: brightness)
    }

    func with(hue next: Double) -> HSBColor {
        HSBColor(hue: next, saturation: saturation, brightness: brightness)
    }

    func with(saturation nextSaturation: Double, brightness nextBrightness: Double) -> HSBColor {
        HSBColor(hue: hue, saturation: nextSaturation, brightness: nextBrightness)
    }
}

extension Color {
    var hexRGB: String {
        guard let ptr = calm_format_hex_rgb(packedRGB) else {
            return String(format: "%06X", packedRGB)
        }
        let hex = String(cString: ptr)
        calm_string_free(ptr)
        return hex
    }

    init?(hexRGB: String) {
        var rgb: UInt32 = 0
        let status = hexRGB.withCString { calm_parse_hex_rgb($0, &rgb) }
        guard status == CalmStatusOk else { return nil }
        self.init(rgb: rgb)
    }
}

struct QuickColorPicker: View {
    @EnvironmentObject private var app: AppModel
    @Environment(\.themeColors) private var colors
    @Environment(\.l10n) private var l10n
    @FocusState private var hexFocused: Bool
    @State private var hexText = ""

    private static let fieldHeight: CGFloat = 84
    private static let sliderHeight: CGFloat = 14
    private static let knob: CGFloat = 10
    private static let swatchHeight: CGFloat = 24

    var body: some View {
        VStack(spacing: Tokens.Space.xs) {
            swatches
            gradientField
            hueSlider
            hexField
        }
        .onAppear { hexText = app.editedColor.hexRGB }
        .onChange(of: app.editedColor) { _, next in
            if !hexFocused {
                hexText = next.hexRGB
            }
        }
    }

    /// The three ink swatches — primary, secondary, tertiary — and nothing else. Each one
    /// points the field, hue slider and hex box at a different color: the picker only ever
    /// edits one thing, and the ring says which. Driven off `quickColors`' own count rather
    /// than a fixed list, so the slots stay in one place.
    ///
    /// There is no separate outline swatch: a shape outlines itself in the primary color and
    /// fills itself with the secondary one, so the two roles are already on screen. A fourth
    /// swatch would have been a fourth color to keep track of for the same result.
    private var swatches: some View {
        HStack(spacing: Tokens.Space.xs) {
            ForEach(Array(app.quickColors.enumerated()), id: \.offset) { index, quick in
                swatch(
                    quick,
                    active: app.activeQuickColorIndex == index,
                    tooltip: inkSwatchTooltip(index)
                ) { app.selectQuickColor(index) }
            }
        }
    }

    /// For the shape tools the first two slots have a job, not just an order, so the tooltip
    /// says which part of a shape each one paints.
    private func inkSwatchTooltip(_ index: Int) -> String {
        switch index {
        case 0: return app.tool.takesFill ? l10n.strokeColor : l10n.primaryColor
        case 1: return app.tool.takesFill ? l10n.fill : l10n.secondaryColor
        default: return l10n.tertiaryColor
        }
    }

    private func swatch(
        _ color: Color,
        active: Bool,
        tooltip: String,
        select: @escaping () -> Void
    ) -> some View {
        let shape = RoundedRectangle(cornerRadius: Tokens.Radius.sm, style: .continuous)
        return Button(action: select) {
            shape
                .fill(color)
                .frame(maxWidth: .infinity)
                .frame(height: Self.swatchHeight)
                .overlay(
                    shape.strokeBorder(
                        active ? colors.accentTeal : colors.islandBorder,
                        lineWidth: active ? 2 : 1
                    )
                )
        }
        .buttonStyle(.plain)
        .calmTooltip(tooltip, edge: .trailing)
        .calmPointer()
    }

    private var gradientField: some View {
        GeometryReader { geo in
            let size = geo.size
            ZStack(alignment: .topLeading) {
                Rectangle().fill(Color(hue: app.hsb.hue, saturation: 1, brightness: 1))
                LinearGradient(
                    colors: [.white, .white.opacity(0)],
                    startPoint: .leading,
                    endPoint: .trailing
                )
                LinearGradient(
                    colors: [.black.opacity(0), .black],
                    startPoint: .top,
                    endPoint: .bottom
                )
                knobCircle
                    .offset(
                        x: app.hsb.saturation * size.width - Self.knob / 2,
                        y: (1 - app.hsb.brightness) * size.height - Self.knob / 2
                    )
            }
            .clipShape(RoundedRectangle(cornerRadius: Tokens.Radius.sm, style: .continuous))
            .contentShape(Rectangle())
            .gesture(
                DragGesture(minimumDistance: 0).onChanged { value in
                    app.updateHSB(app.hsb.with(
                        saturation: Double(value.location.x / max(size.width, 1)),
                        brightness: Double(1 - value.location.y / max(size.height, 1))
                    ))
                }
            )
        }
        .frame(height: Self.fieldHeight)
    }

    private var hueSlider: some View {
        GeometryReader { geo in
            ZStack(alignment: .leading) {
                LinearGradient(colors: Self.hueSpectrum, startPoint: .leading, endPoint: .trailing)
                knobCircle
                    .offset(x: app.hsb.hue * max(geo.size.width - Self.knob, 0))
            }
            .clipShape(RoundedRectangle(cornerRadius: Tokens.Radius.sm, style: .continuous))
            .contentShape(Rectangle())
            .gesture(
                DragGesture(minimumDistance: 0).onChanged { value in
                    app.updateHSB(app.hsb.with(hue: value.location.x / max(geo.size.width, 1)))
                }
            )
        }
        .frame(height: Self.sliderHeight)
    }

    private var knobCircle: some View {
        Circle()
            .fill(.white)
            .frame(width: Self.knob, height: Self.knob)
            .overlay(Circle().strokeBorder(.black.opacity(0.35), lineWidth: 1))
    }

    private var hexField: some View {
        HStack(spacing: 2) {
            CalmText.muted("#", mono: true)
            TextField("", text: $hexText)
                .textFieldStyle(.plain)
                .font(.system(size: Tokens.TypeSize.label, weight: .medium).monospaced())
                .foregroundStyle(colors.text)
                .focused($hexFocused)
                .onSubmit { commitHex() }
        }
        .padding(.horizontal, Tokens.Space.xs)
        .padding(.vertical, 3)
        .calmSurface(radius: Tokens.Radius.sm, bordered: true, focused: hexFocused)
        .help(l10n.hex)
        .onChange(of: hexFocused) { _, focused in
            if !focused {
                commitHex()
            }
        }
    }

    private func commitHex() {
        if let parsed = Color(hexRGB: hexText) {
            app.editedColor = parsed
        }
        hexText = app.editedColor.hexRGB
    }

    private static let hueSpectrum: [Color] = stride(from: 0.0, through: 1.0, by: 1.0 / 11.0)
        .map { Color(hue: $0, saturation: 1, brightness: 1) }
}
