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
        let cell = CalmToolButton.size + spacing
        return Tokens.Space.xs * 2
            + CGFloat(columns) * cell
            + CGFloat(max(0, columns - 1)) * spacing
    }

    static func iconGrid<Content: View>(@ViewBuilder content: () -> Content) -> some View {
        LazyVGrid(columns: gridColumns, spacing: spacing, content: content)
    }

    var body: some View {
        CalmIsland(padding: Tokens.Space.xs) {
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
            .frame(maxHeight: .infinity, alignment: .top)
        }
        .frame(width: Self.width)
    }

    private var toolGrid: some View {
        Self.iconGrid {
            toolButton(.pen) { AppIcon.pen(color: iconColor(.pen)) }
            toolButton(.eraser) { AppIcon.eraser(color: iconColor(.eraser)) }
            shapeToolButton
            selectToolButton
            toolButton(.bucket) { AppIcon.bucket(color: iconColor(.bucket)) }
            toolButton(.eyedropper) { AppIcon.eyedropper(color: iconColor(.eyedropper)) }
            toolButton(.text) { AppIcon.text(color: iconColor(.text)) }
            toolButton(.move) { AppIcon.move(color: iconColor(.move)) }
        }
    }

    private var toolTitle: String {
        if app.tool.isShape { return l10n.shapes }
        if app.tool.isSelection { return l10n.selectionTools }
        switch app.tool {
        case .pen: return l10n.toolPen
        case .eraser: return l10n.toolEraser
        case .bucket: return l10n.toolBucket
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
                }
            }

            if app.tool == .text {
                TextOptions()
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

    private var aiSection: some View {
        Menu {
            Button(l10n.removeBackground) {
                app.engine.removeBackground()
            }
            .disabled(!app.engine.canRemoveBackground)
        } label: {
            HStack(spacing: Tokens.Space.xs) {
                AppIcon.ai(color: colors.textMuted)
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
        case .eyedropper: AppIcon.eyedropper(color: color)
        case .text: AppIcon.text(color: color)
        case .move: AppIcon.move(color: color)
        case .selectRect, .selectEllipse, .selectLasso: AppIcon.selectRect(color: color)
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
        case .line: return l10n.toolLine
        case .rect: return l10n.toolRect
        case .ellipse: return l10n.toolEllipse
        case .arrow: return l10n.toolArrow
        case .triangle: return l10n.toolTriangle
        case .pentagon: return l10n.toolPentagon
        case .selectRect: return l10n.toolSelectRect
        case .selectEllipse: return l10n.toolSelectEllipse
        case .selectLasso: return l10n.toolSelectLasso
        case .eyedropper: return l10n.toolEyedropper
        case .text: return l10n.toolText
        case .move: return l10n.toolMove
        }
    }

    private func iconColor(_ tool: CalmTool) -> Color {
        app.tool == tool ? colors.accentTeal : colors.textMuted
    }
}
