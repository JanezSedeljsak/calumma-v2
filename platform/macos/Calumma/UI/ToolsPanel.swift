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
                            ToolOptions()
                        }

                        Spacer(minLength: Tokens.Space.xs)

                        CalmDivider()

                        VStack(spacing: Tokens.Space.xs) {
                            CalmText.label(l10n.color)
                            QuickColorPicker()
                        }

                        CalmDivider()

                        smartToolsSection
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
            toolButton(.clone) { AppIcon.clone(color: iconColor(.clone)) }
            toolButton(.heal) { AppIcon.heal(color: iconColor(.heal)) }
            shapeToolButton
            toolButton(.bucket) { AppIcon.bucket(color: iconColor(.bucket)) }
            toolButton(.eyedropper) { AppIcon.eyedropper(color: iconColor(.eyedropper)) }
            toolButton(.text) { AppIcon.text(color: iconColor(.text)) }
            toolButton(.crop) { AppIcon.crop(color: iconColor(.crop)) }
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
        case .clone: return l10n.toolClone
        case .heal: return l10n.toolHeal
        case .eyedropper: return l10n.toolEyedropper
        case .text: return l10n.toolText
        case .move: return l10n.toolMove
        case .crop: return l10n.toolCrop
        default: return l10n.toolPen
        }
    }

    private var smartToolsBusy: Bool { app.engine.smartToolBusyLayer != nil }

    private var smartMatteTitle: String {
        if smartToolsBusy { return l10n.smartMatteWorking }
        return app.engine.hasSelection ? l10n.smartMatteDrawn : l10n.smartMatte
    }

    /// Deterministic, no-trained-weights tools — Lanczos-3 upscaling, and (Vision aside) Remove
    /// Background — grouped under one menu the way the AI-only version used to be one button.
    private var smartToolsSection: some View {
        Menu {
            Button(smartToolsBusy ? l10n.removeBackgroundWorking : l10n.removeBackground) {
                app.removeBackground()
            }
            .disabled(!app.engine.canRemoveBackground)
            Button(smartToolsBusy ? l10n.upscaleWorking : l10n.upscale) {
                app.upscale()
            }
            .disabled(!app.engine.canUpscale)
            // The label follows the selection: drawing around the subject is what makes this
            // cut against a real boundary rather than a guess, so the menu says which one it is
            // about to do instead of leaving the difference invisible.
            Button(smartMatteTitle) { app.smartMatte() }
                .disabled(!app.engine.canSmartMatte)
                .help(l10n.smartMatteHint)
            Button(smartToolsBusy ? l10n.seamCarveWorking : l10n.seamCarve) {
                app.seamCarve()
            }
            .disabled(!app.engine.canSeamCarve)
        } label: {
            HStack(spacing: Tokens.Space.xs) {
                if smartToolsBusy {
                    ProgressView()
                        .controlSize(.small)
                        .frame(width: 16, height: 16)
                } else {
                    AppIcon.ai(color: colors.textMuted)
                }
                CalmText.label(l10n.smartTools)
            }
            .frame(maxWidth: .infinity)
            .frame(height: Tokens.Control.height)
            .contentShape(Rectangle())
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .calmTooltip(l10n.smartTools, edge: .trailing)
        .calmPointer()
    }

    /// Move just selects Move: a grab drags the layer and nothing else. Transform is the options
    /// toggle (and `⌘T`), which adds the scale/rotate handles on top — a mode you ask for, not
    /// one that arrives with the tool.
    private var moveButton: some View {
        let selected = app.tool == .move
        let transforming = selected && app.engine.state.transformActive
        return CalmToolButton(
            selected: selected,
            action: { app.selectTool(.move) },
            tooltip: l10n.toolMove,
            shortcut: CalmTool.move.shortcutLabel,
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
            tooltip: app.engine.toolBlock(current).reason(l10n) ?? l10n.shapes,
            shortcut: app.engine.toolShortcut(current),
            tooltipEdge: .trailing,
            enabled: !app.engine.isBlocked(current)
        ) {
            ToolIcon.tool(current, color: active ? colors.accentTeal : colors.textMuted)
        }
    }

    private var selectToolButton: some View {
        let active = app.tool.isSelection
        let current = active ? app.tool : app.lastSelectTool
        return CalmToolButton(
            selected: active,
            action: { app.selectTool(app.lastSelectTool) },
            tooltip: app.engine.toolBlock(current).reason(l10n) ?? l10n.selectionTools,
            shortcut: app.engine.toolShortcut(current),
            tooltipEdge: .trailing,
            enabled: !app.engine.isBlocked(current)
        ) {
            ToolIcon.selection(current, color: active ? colors.accentTeal : colors.textMuted)
        }
    }

    private func toolButton<Icon: View>(_ tool: CalmTool, @ViewBuilder icon: () -> Icon) -> some View {
        CalmToolButton(
            selected: app.tool == tool,
            action: { app.selectTool(tool) },
            tooltip: app.engine.toolTooltip(tool, l10n),
            shortcut: app.engine.toolShortcut(tool),
            tooltipEdge: .trailing,
            enabled: !app.engine.isBlocked(tool)
        ) { icon() }
    }

    private func iconColor(_ tool: CalmTool) -> Color {
        app.tool == tool && !app.engine.isBlocked(tool) ? colors.accentTeal : colors.textMuted
    }
}
