import SwiftUI

struct ToolsPanel: View {
    @EnvironmentObject private var app: AppModel
    @Environment(\.themeColors) private var colors
    @Environment(\.l10n) private var l10n

    
    static let columns = 2
    static let spacing = Tokens.Space.sm
    static var gridColumns: [GridItem] {
        Array(repeating: GridItem(.flexible(), spacing: spacing), count: columns)
    }
    static var width: CGFloat {
        let cell = CalmToolButtonLayout.size + spacing
        return Tokens.Space.xs * 2
            + CGFloat(columns) * cell
            + CGFloat(max(0, columns - 1)) * spacing
    }

    static func iconGrid<Content: View>(@ViewBuilder content: () -> Content) -> some View {
        LazyVGrid(columns: gridColumns, spacing: spacing, content: content)
    }

    var body: some View {
        CalmIsland(padding: Tokens.Space.xs) {
            GeometryReader { proxy in
                ScrollView(.vertical) {
                    VStack(spacing: Tokens.Space.sm) {
                        toolGrid

                        CalmDivider()

                        VStack(spacing: Tokens.Space.xs) {
                            CalmText.label(toolTitle)
                            toolOptions
                        }

                        Spacer(minLength: Tokens.Space.xs)

                        CalmDivider()

                        VStack(spacing: Tokens.Space.xs) {
                            CalmText.label(l10n.color)
                            QuickColorPicker()
                        }

                        CalmDivider()

                        aiSection
                    }
                    .frame(minHeight: proxy.size.height)
                    .calmScrollBars()
                }
            }
        }
        .frame(width: Self.width)
    }

    private var toolGrid: some View {
        Self.iconGrid {
            moveButton
            selectToolButton
            toolButton(.pen) { AppIcon.pen(color: iconColor(.pen)) }
            toolButton(.eraser) { AppIcon.eraser(color: iconColor(.eraser)) }
            toolButton(.blur) { AppIcon.blur(color: iconColor(.blur)) }
            shapeToolButton
            toolButton(.bucket) { AppIcon.bucket(color: iconColor(.bucket)) }
            toolButton(.eyedropper) { AppIcon.eyedropper(color: iconColor(.eyedropper)) }
            toolButton(.text) { AppIcon.text(color: iconColor(.text)) }
        }
    }

    private var toolTitle: String {
        if app.tool.isShape { return l10n.shapes }
        if app.tool.isSelection { return l10n.selectionTools }
        switch app.tool {
        case .pen: return l10n.toolPen
        case .eraser: return l10n.toolEraser
        case .bucket: return l10n.toolBucket
        case .blur: return l10n.toolBlur
        case .eyedropper: return l10n.toolEyedropper
        case .text: return l10n.toolText
        case .move: return l10n.toolMove
        default: return l10n.toolPen
        }
    }

    private var toolOptions: some View {
        VStack(spacing: Tokens.Space.sm) {
            pickerOptions
            sliderOptions
            toggleOptions
        }
    }

    @ViewBuilder
    private var pickerOptions: some View {
        if app.tool.isShape {
            Self.iconGrid {
                shapePick(.line)
                shapePick(.rect)
                shapePick(.ellipse)
                shapePick(.arrow)
                shapePick(.triangle)
                shapePick(.pentagon)
            }
        }
        if app.tool.isSelection {
            Self.iconGrid {
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
            Self.iconGrid {
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
            optionSlider(
                label: l10n.brushSize,
                valueText: "\(Int(app.brushSize))",
                value: Binding(
                    get: { Double(app.brushSize) },
                    set: { app.brushSize = Float($0) }
                ),
                range: 1...96
            )
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
            HStack {
                CalmText.muted(l10n.vectorMode)
                Spacer()
                Toggle("", isOn: $app.vectorMode)
                    .toggleStyle(.switch)
                    .controlSize(.mini)
                    .labelsHidden()
            }
            .help(l10n.vectorModeHint)
        }
    }

    private func optionSlider(
        label: String,
        valueText: String,
        value: Binding<Double>,
        range: ClosedRange<Double>,
        step: Double? = nil
    ) -> some View {
        VStack(spacing: 2) {
            HStack {
                CalmText.muted(label)
                Spacer()
                CalmText.muted(valueText, mono: true)
            }
            slider(value: value, range: range, step: step)
                .controlSize(.mini)
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

    private var aiIsBusy: Bool { app.engine.aiOpBusyLayer != nil }

    private var aiSection: some View {
        Menu {
            Button(aiIsBusy ? l10n.removeBackgroundWorking : l10n.removeBackground) {
                app.removeBackground()
            }
            .disabled(!app.engine.canRemoveBackground)
        } label: {
            HStack(spacing: Tokens.Space.xs) {
                if aiIsBusy {
                    ProgressView()
                        .controlSize(.small)
                        .frame(width: 16, height: 16)
                } else {
                    AppIcon.ai(color: colors.textMuted)
                }
                CalmText.label(l10n.ai)
            }
            .frame(maxWidth: .infinity)
            .frame(height: 28)
            .contentShape(Rectangle())
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .calmTooltip(l10n.aiTools, edge: .trailing)
        .calmPointer()
    }

    /// Move and Transform are one button: moving a layer and resizing one are the same intent
    /// one step apart, so picking Move drops the active layer straight into `⌘T` with its
    /// corners live. Transform is still a *mode* the engine owns — pressing the button while
    /// already inside it steps back out without giving up the tool, and `⌘T` keeps working.
    private var moveButton: some View {
        let selected = app.tool == .move
        let transforming = selected && app.engine.state.transformActive
        return CalmToolButton(
            selected: selected,
            action: {
                if transforming {
                    app.engine.toggleTransform()
                } else {
                    app.enterMoveTransform()
                }
            },
            tooltip: l10n.toolMove,
            tooltipEdge: .trailing
        ) {
            if transforming {
                AppIcon.transform(color: colors.accentTeal)
            } else {
                AppIcon.moveIcon(color: iconColor(.move))
            }
        }
    }

    private var shapeToolButton: some View {
        let active = app.tool.isShape
        let current = active ? app.tool : app.lastShapeTool
        return CalmToolButton(
            selected: active,
            action: { app.selectTool(app.lastShapeTool) },
            tooltip: l10n.shapes,
            tooltipEdge: .trailing
        ) {
            shapeIcon(current, color: active ? colors.accentTeal : colors.textMuted)
        }
    }

    private func brushPick(_ brush: CalmBrush) -> some View {
        CalmToolButton(
            selected: app.brush == brush,
            action: { app.brush = brush },
            tooltip: brushName(brush),
            tooltipEdge: .trailing
        ) {
            brushIcon(brush, color: app.brush == brush ? colors.accentTeal : colors.textMuted)
        }
    }

    @ViewBuilder
    private func brushIcon(_ brush: CalmBrush, color: Color) -> some View {
        switch brush {
        case .pen: AppIcon.pen(color: color)
        case .marker: AppIcon.marker(color: color)
        case .crayon: AppIcon.crayon(color: color)
        case .airbrush: AppIcon.airbrush(color: color)
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
            tooltip: toolHelp(tool),
            tooltipEdge: .trailing
        ) {
            shapeIcon(tool, color: app.tool == tool ? colors.accentTeal : colors.textMuted)
        }
    }

    @ViewBuilder
    private func shapeIcon(_ tool: CalmTool, color: Color) -> some View {
        switch tool {
        case .line: AppIcon.line(color: color)
        case .rect: AppIcon.shape(color: color)
        case .ellipse: AppIcon.ellipse(color: color)
        case .arrow: AppIcon.arrow(color: color)
        case .triangle: AppIcon.triangle(color: color)
        case .pentagon: AppIcon.pentagon(color: color)
        case .pen: AppIcon.pen(color: color)
        case .eraser: AppIcon.eraser(color: color)
        case .bucket: AppIcon.bucket(color: color)
        case .blur: AppIcon.blur(color: color)
        case .eyedropper: AppIcon.eyedropper(color: color)
        case .text: AppIcon.text(color: color)
        case .move: AppIcon.moveIcon(color: color)
        case .selectRect, .selectEllipse, .selectLasso: AppIcon.selectRect(color: color)
        case .magicWand: AppIcon.magicWand(color: color)
        }
    }

    private var selectToolButton: some View {
        let active = app.tool.isSelection
        let current = active ? app.tool : app.lastSelectTool
        return CalmToolButton(
            selected: active,
            action: { app.selectTool(app.lastSelectTool) },
            tooltip: l10n.selectionTools,
            tooltipEdge: .trailing
        ) {
            selectionIcon(current, color: active ? colors.accentTeal : colors.textMuted)
        }
    }

    private func selectPick(_ tool: CalmTool) -> some View {
        CalmToolButton(
            selected: app.tool == tool,
            action: { app.selectTool(tool) },
            tooltip: toolHelp(tool),
            tooltipEdge: .trailing
        ) {
            selectionIcon(tool, color: app.tool == tool ? colors.accentTeal : colors.textMuted)
        }
    }

    @ViewBuilder
    private func selectionIcon(_ tool: CalmTool, color: Color) -> some View {
        switch tool {
        case .selectEllipse: AppIcon.selectEllipse(color: color)
        case .selectLasso: AppIcon.selectLasso(color: color)
        case .magicWand: AppIcon.magicWand(color: color)
        default: AppIcon.selectRect(color: color)
        }
    }

    private func toolButton<Icon: View>(_ tool: CalmTool, @ViewBuilder icon: () -> Icon) -> some View {
        CalmToolButton(
            selected: app.tool == tool,
            action: { app.selectTool(tool) },
            tooltip: toolHelp(tool),
            tooltipEdge: .trailing
        ) { icon() }
    }

    private func toolHelp(_ tool: CalmTool) -> String {
        switch tool {
        case .pen: return l10n.toolPen
        case .eraser: return l10n.toolEraser
        case .bucket: return l10n.toolBucket
        case .blur: return l10n.toolBlur
        case .line: return l10n.toolLine
        case .rect: return l10n.toolRect
        case .ellipse: return l10n.toolEllipse
        case .arrow: return l10n.toolArrow
        case .triangle: return l10n.toolTriangle
        case .pentagon: return l10n.toolPentagon
        case .selectRect: return l10n.toolSelectRect
        case .selectEllipse: return l10n.toolSelectEllipse
        case .selectLasso: return l10n.toolSelectLasso
        case .magicWand: return l10n.toolMagicWand
        case .eyedropper: return l10n.toolEyedropper
        case .text: return l10n.toolText
        case .move: return l10n.toolMove
        }
    }

    private func iconColor(_ tool: CalmTool) -> Color {
        app.tool == tool ? colors.accentTeal : colors.textMuted
    }
}
