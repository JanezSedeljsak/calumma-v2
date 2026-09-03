import SwiftUI

/// Everything a layer can do, behind the row's one `…` button. The row itself carries only a
/// thumbnail and a name now: visibility, lock, rename and delete moved in here rather than
/// competing for a 276pt-wide row with the four controls that were already in it.
struct LayerSettingsCard: View {
    let index: Int
    let canMoveUp: Bool
    let canMoveDown: Bool
    let canMergeDown: Bool
    let canClipDown: Bool
    let canRename: Bool
    let canDelete: Bool
    /// Renaming happens inline in the row, and deleting has to close this popover before the
    /// index it is bound to goes away — both belong to the panel, so both are handed in.
    let onRename: () -> Void
    let onDelete: () -> Void

    @EnvironmentObject private var app: AppModel
    @Environment(\.themeColors) private var colors
    @Environment(\.l10n) private var l10n

    private static let actionColumns = [
        GridItem(.flexible(), spacing: Tokens.Space.sm),
        GridItem(.flexible(), spacing: Tokens.Space.sm),
    ]

    var body: some View {
        VStack(alignment: .leading, spacing: Tokens.Space.md) {
            // First, because hiding a layer is the most frequent thing anyone does in a layers
            // panel and it is now one click deeper than it was. Toggles rather than buttons:
            // they carry state, so they show it.
            VStack(alignment: .leading, spacing: Tokens.Space.xs) {
                stateToggle(l10n.layerVisibility, isOn: visible) {
                    app.engine.setLayerVisible(index, visible: $0)
                }
                stateToggle(locked ? l10n.layerUnlock : l10n.layerLock, isOn: locked) {
                    app.engine.setLayerLocked(index, locked: $0)
                }
            }

            CalmDivider()

            LazyVGrid(columns: Self.actionColumns, alignment: .leading, spacing: Tokens.Space.sm) {
                if canRename {
                    actionButton(l10n.renameLayer, action: onRename)
                }
                actionButton(l10n.moveLayerUp, enabled: canMoveUp) {
                    app.engine.moveLayerUp(index)
                }
                actionButton(l10n.moveLayerDown, enabled: canMoveDown) {
                    app.engine.moveLayerDown(index)
                }
                actionButton(l10n.copyLayer) {
                    app.copyLayer(index: index)
                }
                actionButton(l10n.exportLayer) {
                    app.exportLayer(index: index)
                }
                actionButton(l10n.duplicateLayer) {
                    app.engine.duplicateLayer(index)
                }
                if canMergeDown {
                    actionButton(l10n.mergeLayerDown) {
                        app.engine.mergeLayerDown(index)
                    }
                }
                if canClipDown {
                    actionButton(l10n.clipLayerDown) {
                        app.engine.clipLayerDown(index)
                    }
                }
                if app.engine.isLayerRasterizable(index: index) {
                    actionButton(l10n.layerRasterize) {
                        app.engine.rasterizeLayer(index)
                    }
                }
                actionButton(l10n.resetTransform) {
                    app.engine.resetLayerTransform(index)
                }
            }

            VStack(alignment: .leading, spacing: Tokens.Space.xs) {
                HStack {
                    CalmText.label(l10n.opacity)
                    Spacer()
                    CalmText.muted("\(Int(opacity * 100))%", mono: true)
                }
                CalmDeferredSlider(
                    value: opacity,
                    range: 0...1,
                    onSettled: { app.engine.setLayerOpacity(index, $0) }
                )
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

            if canDelete {
                CalmDivider()
                // Last and on its own, away from Duplicate — the two are one slip apart
                // otherwise, and this one is the destructive half.
                CalmPlainButton(
                    title: l10n.deleteLayer,
                    fill: true,
                    tint: colors.danger,
                    action: onDelete
                )
            }
        }
        .padding(Tokens.Space.md)
        .frame(width: 260)
        .background(colors.surface)
    }

    private var visible: Bool {
        app.engine.layerVisibles.indices.contains(index) ? app.engine.layerVisibles[index] : true
    }

    private var locked: Bool {
        app.engine.layerLocked.indices.contains(index) ? app.engine.layerLocked[index] : false
    }

    private func stateToggle(
        _ title: String,
        isOn: Bool,
        set: @escaping (Bool) -> Void
    ) -> some View {
        HStack {
            CalmText.muted(title)
            Spacer()
            Toggle("", isOn: Binding(get: { isOn }, set: set))
                .toggleStyle(.switch)
                .controlSize(.mini)
                .labelsHidden()
        }
    }

    private func actionButton(
        _ title: String,
        enabled: Bool = true,
        action: @escaping () -> Void
    ) -> some View {
        CalmPlainButton(title: title, enabled: enabled, fill: true, action: action)
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
            CalmDeferredSlider(value: value, range: range, onSettled: onChange)
        }
    }
}
