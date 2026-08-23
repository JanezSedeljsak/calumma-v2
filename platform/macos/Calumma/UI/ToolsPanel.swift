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
            toolButton(.move) { AppIcon.moveIcon(color: iconColor(.move)) }
            toolButton(.pen) { AppIcon.pen(color: iconColor(.pen)) }
            toolButton(.eraser) { AppIcon.eraser(color: iconColor(.eraser)) }
            toolButton(.blur) { AppIcon.blur(color: iconColor(.blur)) }
            shapeToolButton
            selectToolButton
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

    @ViewBuilder
    private var toolOptions: some View {
        VStack(spacing: Tokens.Space.sm) {
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

            if showsBrushSize {
                VStack(spacing: 2) {
                    HStack {
                        CalmText.muted(l10n.brushSize)
                        Spacer()
                        CalmText.muted("\(Int(app.brushSize))", mono: true)
                    }
                    Slider(value: Binding(
                        get: { Double(app.brushSize) },
                        set: { app.brushSize = Float($0) }
                    ), in: 1...96)
                    .controlSize(.mini)
                }
            }

            if showsInkOpacity {
                VStack(spacing: 2) {
                    HStack {
                        CalmText.muted(l10n.inkOpacity)
                        Spacer()
                        CalmText.muted("\(Int((app.inkOpacity * 100).rounded()))", mono: true)
                    }
                    Slider(value: Binding(
                        get: { Double(app.inkOpacity) },
                        set: { app.inkOpacity = Float($0) }
                    ), in: Double(Engine.inkOpacityMin)...Double(Engine.inkOpacityMax))
                    .controlSize(.mini)
                }
            }

            if showsEraserHardness {
                VStack(spacing: 2) {
                    HStack {
                        CalmText.muted(l10n.eraserHardness)
                        Spacer()
                        CalmText.muted("\(Int((app.eraserHardness * 100).rounded()))", mono: true)
                    }
                    Slider(value: Binding(
                        get: { Double(app.eraserHardness) },
                        set: { app.eraserHardness = Float($0) }
                    ), in: Double(Engine.eraserHardnessMin)...Double(Engine.eraserHardnessMax))
                    .controlSize(.mini)
                }
            }

            if showsBlurStrength {
                VStack(spacing: 2) {
                    HStack {
                        CalmText.muted(l10n.blurStrength)
                        Spacer()
                        CalmText.muted("\(Int((app.blurStrength * 100).rounded()))", mono: true)
                    }
                    Slider(value: Binding(
                        get: { Double(app.blurStrength) },
                        set: { app.blurStrength = Float($0) }
                    ), in: Double(Engine.blurStrengthMin)...Double(Engine.blurStrengthMax))
                    .controlSize(.mini)
                }
            }

            if showsTolerance {
                VStack(spacing: 2) {
                    HStack {
                        CalmText.muted(l10n.tolerance)
                        Spacer()
                        CalmText.muted("\(app.tolerance)", mono: true)
                    }
                    Slider(value: Binding(
                        get: { Double(app.tolerance) },
                        set: { app.tolerance = UInt8($0.rounded()) }
                    ), in: Double(Engine.toleranceMin)...Double(Engine.toleranceMax))
                    .controlSize(.mini)
                }
            }

            if app.tool.isShape {
                HStack {
                    CalmText.muted(l10n.fill)
                    Spacer()
                    Toggle("", isOn: $app.fill)
                        .toggleStyle(.switch)
                        .controlSize(.mini)
                        .labelsHidden()
                }
                .help(l10n.fill)
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
