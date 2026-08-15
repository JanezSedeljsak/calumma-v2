import SwiftUI

struct LayerSettingsCard: View {
    let index: Int
    let canMergeDown: Bool

    @EnvironmentObject private var app: AppModel
    @Environment(\.themeColors) private var colors
    @Environment(\.l10n) private var l10n

    var body: some View {
        VStack(alignment: .leading, spacing: Tokens.Space.md) {
            HStack(spacing: Tokens.Space.sm) {
                CalmPlainButton(title: l10n.copyLayer) {
                    app.copyLayer(index: index)
                }
                CalmPlainButton(title: l10n.exportLayer) {
                    app.exportLayer(index: index)
                }
                CalmPlainButton(title: l10n.duplicateLayer) {
                    app.engine.duplicateLayer(index)
                }
                if canMergeDown {
                    CalmPlainButton(title: l10n.mergeLayerDown) {
                        app.engine.mergeLayerDown(index)
                    }
                }
                if app.engine.isLayerText(index: index) {
                    CalmPlainButton(title: l10n.layerRasterizeText) {
                        app.engine.rasterizeTextLayer(index)
                    }
                }
                CalmPlainButton(title: l10n.resetTransform) {
                    app.engine.resetLayerTransform(index)
                }
            }

            VStack(alignment: .leading, spacing: Tokens.Space.xs) {
                HStack {
                    CalmText.label(l10n.opacity)
                    Spacer()
                    CalmText.muted("\(Int(opacity * 100))%", mono: true)
                }
                Slider(value: Binding(
                    get: { Double(opacity) },
                    set: { app.engine.setLayerOpacity(index, Float($0)) }
                ), in: 0...1)
                .controlSize(.mini)
            }

            VStack(alignment: .leading, spacing: Tokens.Space.xs) {
                CalmText.label(l10n.blendMode)
                Picker(l10n.blendMode, selection: Binding(
                    get: { blendMode },
                    set: { app.engine.setLayerBlendMode(index, $0) }
                )) {
                    Text(l10n.blendNormal).tag(CalmBlendMode.normal)
                    Text(l10n.blendMultiply).tag(CalmBlendMode.multiply)
                    Text(l10n.blendScreen).tag(CalmBlendMode.screen)
                }
                .pickerStyle(.segmented)
                .labelsHidden()
            }

            Divider()

            VStack(alignment: .leading, spacing: Tokens.Space.sm) {
                HStack {
                    CalmText.label(l10n.filters)
                    Spacer()
                    CalmPlainButton(title: l10n.resetFilters) {
                        app.engine.setLayerAdjustments(index, LayerAdjustments())
                    }
                }
                filterSlider(
                    l10n.brightness, value: adjustments.brightness, range: -1...1,
                    onChange: update { $0.brightness = $1 }
                )
                filterSlider(
                    l10n.contrast, value: adjustments.contrast, range: -1...1,
                    onChange: update { $0.contrast = $1 }
                )
                filterSlider(
                    l10n.vibrance, value: adjustments.vibrance, range: -1...1,
                    onChange: update { $0.vibrance = $1 }
                )
                filterSlider(
                    l10n.saturation, value: adjustments.saturation, range: -1...1,
                    onChange: update { $0.saturation = $1 }
                )
                filterSlider(
                    l10n.levelsGamma, value: adjustments.levelsGamma, range: 0.1...4,
                    onChange: update { $0.levelsGamma = $1 }
                )
            }
        }
        .padding(Tokens.Space.md)
        .frame(width: 260)
        .background(colors.surface)
    }

    private var opacity: Float {
        app.engine.layerOpacities.indices.contains(index) ? app.engine.layerOpacities[index] : 1
    }

    private var blendMode: CalmBlendMode {
        app.engine.layerBlendModes.indices.contains(index) ? app.engine.layerBlendModes[index] : .normal
    }

    private var adjustments: LayerAdjustments {
        app.engine.layerAdjustments.indices.contains(index)
            ? app.engine.layerAdjustments[index]
            : LayerAdjustments()
    }

    private func update(_ mutate: @escaping (inout LayerAdjustments, Float) -> Void) -> (Float) -> Void {
        { value in
            var next = adjustments
            mutate(&next, value)
            app.engine.setLayerAdjustments(index, next)
        }
    }

    private func filterSlider(
        _ label: String,
        value: Float,
        range: ClosedRange<Float>,
        onChange: @escaping (Float) -> Void
    ) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            CalmText.muted(label)
            Slider(value: Binding(
                get: { Double(value) },
                set: { onChange(Float($0)) }
            ), in: Double(range.lowerBound)...Double(range.upperBound))
            .controlSize(.mini)
        }
    }
}
