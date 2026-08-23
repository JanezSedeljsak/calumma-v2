import SwiftUI

/// Text-tool options: which family, how big, how bold or slanted, how loose, how aligned.
///
/// The family list comes from the engine, not from `NSFontManager` — the engine holds the
/// font database that rasterizes the glyphs, so it is the only source that cannot offer a
/// family the board would then fail to draw.
struct TextOptions: View {
    @EnvironmentObject private var app: AppModel
    @Environment(\.themeColors) private var colors
    @Environment(\.l10n) private var l10n

    @State private var pickerOpen = false

    var body: some View {
        VStack(spacing: Tokens.Space.sm) {
            familyButton
            sizeSlider
            lineHeightSlider
            styleRow
            alignRow
        }
    }

    private var familyButton: some View {
        Button {
            pickerOpen = true
        } label: {
            HStack(spacing: Tokens.Space.xs) {
                Text(app.engine.textFamily)
                    .font(.system(size: Tokens.TypeSize.label))
                    .lineLimit(1)
                    .truncationMode(.tail)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, Tokens.Space.xs)
            .frame(height: 24)
            .frame(maxWidth: .infinity)
            .calmSurface(radius: Tokens.Radius.sm)
            .foregroundStyle(colors.text)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .calmPointer()
        .calmTooltip(l10n.textFont, edge: .trailing)
        .popover(isPresented: $pickerOpen, arrowEdge: .trailing) {
            FontFamilyPicker(selected: app.engine.textFamily) { family in
                app.engine.setTextFamily(family)
                pickerOpen = false
            }
        }
    }

    private var sizeSlider: some View {
        VStack(spacing: 2) {
            HStack {
                CalmText.muted(l10n.textSize)
                Spacer()
                CalmText.muted("\(Int(app.engine.textSize))", mono: true)
            }
            Slider(
                value: Binding(
                    get: { Double(app.engine.textSize) },
                    set: { app.engine.setTextSize(Float($0)) }
                ),
                in: Double(Engine.textSizeRange.lowerBound)...Double(
                    Engine.textSizeRange.upperBound
                )
            )
            .controlSize(.mini)
        }
    }

    private var lineHeightSlider: some View {
        VStack(spacing: 2) {
            HStack {
                CalmText.muted(l10n.textLineHeight)
                Spacer()
                CalmText.muted(String(format: "%.2f", app.engine.textLineHeight), mono: true)
            }
            Slider(
                value: Binding(
                    get: { Double(app.engine.textLineHeight) },
                    set: { app.engine.setTextLineHeight(Float($0)) }
                ),
                in: Double(Engine.textLineHeightRange.lowerBound)...Double(
                    Engine.textLineHeightRange.upperBound
                )
            )
            .controlSize(.mini)
        }
    }

    /// Bold and italic are offered only where the family really has that cut — the engine
    /// reports which faces it loaded, and a synthesised slant is not the font anyone picked.
    private var styleRow: some View {
        let family = app.engine.activeFontFamily
        return ToolsPanel.iconGrid {
            styleButton(
                glyph: "bold",
                help: l10n.textBold,
                selected: app.engine.textBold,
                available: family?.hasBold ?? false,
                action: { app.engine.setTextBold(!app.engine.textBold) }
            )
            styleButton(
                glyph: "italic",
                help: l10n.textItalic,
                selected: app.engine.textItalic,
                available: family?.hasItalic ?? false,
                action: { app.engine.setTextItalic(!app.engine.textItalic) }
            )
        }
    }

    private func styleButton(
        glyph: String,
        help: String,
        selected: Bool,
        available: Bool,
        action: @escaping () -> Void
    ) -> some View {
        CalmToolButton(
            selected: selected && available,
            action: action,
            tooltip: help,
            tooltipEdge: .trailing
        ) {
            SvgIcon(name: glyph, color: styleColor(selected: selected, available: available))
        }
        .disabled(!available)
        .opacity(available ? 1 : 0.4)
    }

    private func styleColor(selected: Bool, available: Bool) -> Color {
        guard available else { return colors.textMuted }
        return selected ? colors.accentTeal : colors.textMuted
    }

    private var alignRow: some View {
        ToolsPanel.iconGrid {
            alignButton(.left, glyph: "align-left", help: l10n.textAlignLeft)
            alignButton(.center, glyph: "align-center", help: l10n.textAlignCenter)
            alignButton(.right, glyph: "align-right", help: l10n.textAlignRight)
        }
    }

    private func alignButton(_ align: CalmTextAlign, glyph: String, help: String) -> some View {
        let selected = app.engine.textAlign == align
        return CalmToolButton(
            selected: selected,
            action: { app.engine.setTextAlign(align) },
            tooltip: help,
            tooltipEdge: .trailing
        ) {
            SvgIcon(name: glyph, color: selected ? colors.accentTeal : colors.textMuted)
        }
    }
}

/// Hundreds of families is too many to scroll blind, so the picker filters as you type and
/// previews each row in its own face.
private struct FontFamilyPicker: View {
    @Environment(\.themeColors) private var colors
    @Environment(\.l10n) private var l10n
    let selected: String
    let choose: (String) -> Void

    @State private var query = ""

    private var matches: [CalmFontFamily] {
        let trimmed = query.trimmingCharacters(in: .whitespaces)
        guard !trimmed.isEmpty else { return Engine.fontFamilies }
        return Engine.fontFamilies.filter { $0.name.localizedCaseInsensitiveContains(trimmed) }
    }

    var body: some View {
        VStack(spacing: Tokens.Space.xs) {
            CalmField(text: $query)
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    ForEach(matches) { family in
                        Button {
                            choose(family.name)
                        } label: {
                            HStack {
                                Text(family.name)
                                    .font(.custom(family.name, size: 13))
                                    .lineLimit(1)
                                Spacer(minLength: 0)
                            }
                            .padding(.horizontal, Tokens.Space.sm)
                            .padding(.vertical, Tokens.Space.xs)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .calmSurface(hover: family.name == selected, radius: Tokens.Radius.sm)
                            .foregroundStyle(
                                family.name == selected ? colors.accentTeal : colors.text
                            )
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .calmPointer()
                    }
                }
                .calmScrollBars()
            }
            .frame(height: 280)
            if matches.isEmpty {
                CalmText.muted(l10n.textNoFonts)
            }
        }
        .padding(Tokens.Space.sm)
        .frame(width: 260)
    }
}
