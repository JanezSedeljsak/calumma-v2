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
        String(format: "%06X", packedRGB)
    }

    init?(hexRGB: String) {
        var cleaned = hexRGB.trimmingCharacters(in: .whitespacesAndNewlines).uppercased()
        if cleaned.hasPrefix("#") {
            cleaned.removeFirst()
        }
        if cleaned.count == 3 {
            cleaned = cleaned.map { "\($0)\($0)" }.joined()
        }
        guard cleaned.count == 6, let value = UInt32(cleaned, radix: 16) else { return nil }
        self.init(rgb: value)
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

    var body: some View {
        VStack(spacing: Tokens.Space.xs) {
            HStack(spacing: Tokens.Space.xs) {
                swatch(0)
                swatch(1)
            }
            gradientField
            hueSlider
            hexField
            ColorPicker("", selection: $app.color, supportsOpacity: true)
                .labelsHidden()
                .frame(height: 24)
                .clipShape(RoundedRectangle(cornerRadius: Tokens.Radius.sm, style: .continuous))
                .help(l10n.color)
        }
        .onAppear { hexText = app.color.hexRGB }
        .onChange(of: app.color) { _, next in
            if !hexFocused {
                hexText = next.hexRGB
            }
        }
    }

    private func swatch(_ index: Int) -> some View {
        let shape = RoundedRectangle(cornerRadius: Tokens.Radius.sm, style: .continuous)
        return Button {
            app.selectQuickColor(index)
        } label: {
            shape
                .fill(app.quickColors[index])
                .frame(maxWidth: .infinity)
                .frame(height: 22)
                .overlay(
                    shape.strokeBorder(
                        colors.accentTeal,
                        lineWidth: app.activeQuickColorIndex == index ? 2 : 0
                    )
                )
        }
        .buttonStyle(.plain)
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
        .calmSurface(radius: Tokens.Radius.sm)
        .help(l10n.hex)
        .onChange(of: hexFocused) { _, focused in
            if !focused {
                commitHex()
            }
        }
    }

    private func commitHex() {
        if let parsed = Color(hexRGB: hexText) {
            app.color = parsed
        }
        hexText = app.color.hexRGB
    }

    private static let hueSpectrum: [Color] = stride(from: 0.0, through: 1.0, by: 1.0 / 11.0)
        .map { Color(hue: $0, saturation: 1, brightness: 1) }
}
