import SwiftUI

/// The contextual half of the tools island: everything under the tool grid that changes with
/// the selected tool — the shape / marquee / brush sub-pickers, the sliders, and the toggles.
/// Split from `ToolsPanel` because the grid answers "which tool" and this answers "set up how",
/// and only one of the two changes when a tool is picked.
struct ToolOptions: View {
    @EnvironmentObject private var app: AppModel
    @Environment(\.themeColors) private var colors
    @Environment(\.l10n) private var l10n

    var body: some View {
        VStack(spacing: Tokens.Space.sm) {
            pickerOptions
            sliderOptions
            toggleOptions
        }
    }


    @ViewBuilder
    private var pickerOptions: some View {
        if app.tool.isShape {
            ToolsPanel.iconGrid {
                shapePick(.line)
                shapePick(.rect)
                shapePick(.ellipse)
                shapePick(.arrow)
                shapePick(.triangle)
                shapePick(.pentagon)
            }
        }
        if app.tool.isSelection {
            ToolsPanel.iconGrid {
                selectPick(.selectRect)
                selectPick(.selectEllipse)
                selectPick(.selectLasso)
                selectPick(.magicWand)
            }
        }
        if app.tool == .text {
            TextOptions()
        }
        if showsBrush {
            ToolsPanel.iconGrid {
                brushPick(.pen)
                brushPick(.marker)
                brushPick(.crayon)
                brushPick(.airbrush)
            }
        }
    }

    @ViewBuilder
    private var sliderOptions: some View {
        if showsBrushSize {
            // The slider runs on 0...1 of travel and the engine places it on the size curve;
            // the field beside it is the way to an exact size the thumb cannot resolve.
            optionSlider(
                label: l10n.brushSize,
                value: Binding(
                    get: { Double(Engine.brushSizeUnit(app.brushSize)) },
                    set: { app.brushSize = Engine.brushSize(fromUnit: Float($0)) }
                ),
                range: 0...1
            ) {
                CalmSliderValueField(
                    value: $app.brushSize,
                    range: Engine.brushSizeMin...Engine.brushSizeMax
                )
            }
        }
        if showsInkOpacity {
            optionSlider(
                label: l10n.inkOpacity,
                valueText: "\(Int((app.inkOpacity * 100).rounded()))",
                value: Binding(
                    get: { Double(app.inkOpacity) },
                    set: { app.inkOpacity = Float($0) }
                ),
                range: Double(Engine.inkOpacityMin)...Double(Engine.inkOpacityMax)
            )
        }
        if showsEraserHardness {
            optionSlider(
                label: l10n.eraserHardness,
                valueText: "\(Int((app.eraserHardness * 100).rounded()))",
                value: Binding(
                    get: { Double(app.eraserHardness) },
                    set: { app.eraserHardness = Float($0) }
                ),
                range: Double(Engine.eraserHardnessMin)...Double(Engine.eraserHardnessMax)
            )
        }
        if showsBlurStrength {
            optionSlider(
                label: l10n.blurStrength,
                valueText: "\(Int((app.blurStrength * 100).rounded()))",
                value: Binding(
                    get: { Double(app.blurStrength) },
                    set: { app.blurStrength = Float($0) }
                ),
                range: Double(Engine.blurStrengthMin)...Double(Engine.blurStrengthMax)
            )
        }
        if showsTolerance {
            optionSlider(
                label: l10n.tolerance,
                valueText: "\(app.tolerance)",
                value: Binding(
                    get: { Double(app.tolerance) },
                    set: { app.tolerance = UInt8($0.rounded()) }
                ),
                range: Double(Engine.toleranceMin)...Double(Engine.toleranceMax)
            )
        }
        if showsEyedropperRadius {
            eyedropperRadiusSlider
        }
    }

    private var eyedropperSampleSide: Int {
        Int(app.eyedropperRadius) * 2 + 1
    }

    private var eyedropperRadiusSlider: some View {
        VStack(spacing: 2) {
            optionSlider(
                label: l10n.sampleSize,
                valueText: "\(eyedropperSampleSide)×\(eyedropperSampleSide)",
                value: Binding(
                    get: { Double(app.eyedropperRadius) },
                    set: { app.eyedropperRadius = UInt32($0.rounded()) }
                ),
                range: Double(Engine.eyedropperRadiusMin)...Double(Engine.eyedropperRadiusMax),
                step: 1
            )
            eyedropperRadiusPreview
        }
    }

    @ViewBuilder
    private var toggleOptions: some View {
        if app.tool.takesFill {
            paintToggle(l10n.fill, isOn: $app.fill)
            paintToggle(l10n.stroke, isOn: $app.stroke)
        }
        if showsVectorMode {
            let pinned = app.engine.vectorModeLocked
            HStack {
                CalmText.muted(l10n.vectorMode)
                Spacer()
                // A vector layer decides this for the pen and the shapes, so the switch shows
                // the decision rather than a knob that is quietly overruled.
                Toggle("", isOn: pinned ? .constant(true) : $app.vectorMode)
                    .toggleStyle(.switch)
                    .controlSize(.mini)
                    .labelsHidden()
                    .disabled(pinned)
            }
            .help(pinned ? l10n.vectorModeLockedHint : l10n.vectorModeHint)
        }
        if app.tool == .move {
            let block = app.engine.toolBlock(.transform)
            VStack(alignment: .leading, spacing: 2) {
                CalmText.muted(l10n.toolTransform)
                Toggle(
                    "",
                    isOn: Binding(
                        get: { app.engine.state.transformActive },
                        set: { app.setMoveTransform($0) }
                    )
                )
                .toggleStyle(.switch)
                .controlSize(.mini)
                .labelsHidden()
                .disabled(block.blocks && !app.engine.state.transformActive)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .help(block.reason(l10n) ?? l10n.transformModeHint)
        }
    }

    /// A slider row is a label, a value, and the track. The value is usually printed —
    /// a percentage or a sample size has nothing to type — but a size row hands in a field
    /// instead, which is the only difference between the two.
    private func optionSlider<Value: View>(
        label: String,
        value: Binding<Double>,
        range: ClosedRange<Double>,
        step: Double? = nil,
        @ViewBuilder trailing: () -> Value
    ) -> some View {
        VStack(spacing: 2) {
            HStack {
                CalmText.muted(label)
                Spacer()
                trailing()
            }
            slider(value: value, range: range, step: step)
                .controlSize(.mini)
        }
    }

    private func optionSlider(
        label: String,
        valueText: String,
        value: Binding<Double>,
        range: ClosedRange<Double>,
        step: Double? = nil
    ) -> some View {
        optionSlider(label: label, value: value, range: range, step: step) {
            CalmText.muted(valueText, mono: true)
        }
    }

    @ViewBuilder
    private func slider(
        value: Binding<Double>,
        range: ClosedRange<Double>,
        step: Double?
    ) -> some View {
        if let step {
            Slider(value: value, in: range, step: step)
        } else {
            Slider(value: value, in: range)
        }
    }

    /// Fill and stroke are independent, so they are two rows rather than one three-state
    /// control — a shape can carry both, and the panel has to be able to say so.
    private func paintToggle(_ label: String, isOn: Binding<Bool>) -> some View {
        HStack {
            CalmText.muted(label)
            Spacer()
            Toggle("", isOn: isOn)
                .toggleStyle(.switch)
                .controlSize(.mini)
                .labelsHidden()
        }
        .help(label)
    }

    private var showsVectorMode: Bool {
        app.tool.showsVectorMode
    }

    private var showsBrushSize: Bool {
        app.tool.takesBrushSize
    }

    private var showsInkOpacity: Bool {
        app.tool.takesInkOpacity
    }

    private var showsBlurStrength: Bool {
        app.tool.takesBlurStrength
    }

    private var showsTolerance: Bool {
        app.tool.takesTolerance
    }

    private var showsEyedropperRadius: Bool {
        app.tool.takesEyedropperRadius
    }

    private var eyedropperRadiusPreview: some View {
        let maxRadius = CGFloat(max(Engine.eyedropperRadiusMax, 1))
        let t = CGFloat(app.eyedropperRadius) / maxRadius
        let diameter = 8 + t * 20
        return Circle()
            .fill(app.color.opacity(0.35))
            .overlay {
                Circle().strokeBorder(colors.textMuted, lineWidth: 1)
            }
            .frame(width: diameter, height: diameter)
            .frame(height: 28)
    }

    /// A vector stroke has no pixels to shape, so the picker would be lying about what it
    /// does — it hides rather than sitting there inert.
    private var showsBrush: Bool {
        app.tool.takesBrush && !app.vectorMode
    }

    private var showsEraserHardness: Bool {
        app.tool.takesEraserHardness
    }

    private func brushPick(_ brush: CalmBrush) -> some View {
        CalmToolButton(
            selected: app.brush == brush,
            action: { app.brush = brush },
            tooltip: brushName(brush),
            tooltipEdge: .trailing
        ) {
            ToolIcon.brush(brush, color: app.brush == brush ? colors.accentTeal : colors.textMuted)
        }
    }

    private func brushName(_ brush: CalmBrush) -> String {
        switch brush {
        case .pen: return l10n.brushPen
        case .marker: return l10n.brushMarker
        case .crayon: return l10n.brushCrayon
        case .airbrush: return l10n.brushAirbrush
        }
    }

    private func shapePick(_ tool: CalmTool) -> some View {
        CalmToolButton(
            selected: app.tool == tool,
            action: { app.selectTool(tool) },
            tooltip: app.engine.toolTooltip(tool, l10n),
            shortcut: app.engine.toolShortcut(tool),
            tooltipEdge: .trailing,
            enabled: !app.engine.isBlocked(tool)
        ) {
            ToolIcon.tool(tool, color: app.tool == tool ? colors.accentTeal : colors.textMuted)
        }
    }

    private func selectPick(_ tool: CalmTool) -> some View {
        CalmToolButton(
            selected: app.tool == tool,
            action: { app.selectTool(tool) },
            tooltip: app.engine.toolTooltip(tool, l10n),
            shortcut: app.engine.toolShortcut(tool),
            tooltipEdge: .trailing,
            enabled: !app.engine.isBlocked(tool)
        ) {
            ToolIcon.selection(tool, color: app.tool == tool ? colors.accentTeal : colors.textMuted)
        }
    }
}
