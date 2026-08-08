import AppKit
import SwiftUI

struct EditorView: View {
    @EnvironmentObject private var app: AppModel
    @Environment(\.themeColors) private var colors
    @Environment(\.l10n) private var l10n
    @State private var hoveredLayer: Int?
    @State private var editingTab: String?
    @State private var artworkDropTargeted = false
    @State private var canvasWidth = 0
    @State private var canvasHeight = 0
    @State private var layerSettingsIndex: Int?

    var body: some View {
        HStack(alignment: .top, spacing: Tokens.Space.sm) {
            ToolsPanel()
                .frame(maxHeight: .infinity)

            canvasIsland
                .frame(maxWidth: .infinity, maxHeight: .infinity)

            if app.layersOpen {
                layersIsland
                    .frame(maxHeight: .infinity)
            }
        }
        .padding(Tokens.Space.sm)
        .calmScreen()
        .toolbar { editorToolbar }
        .onAppear { app.applyKnobs() }
        .onChange(of: app.tool) { _, next in
            if next.isShape { app.lastShapeTool = next }
            app.applyKnobs()
        }
        .onChange(of: app.color) { _, _ in app.applyKnobs() }
        .onChange(of: app.brushSize) { _, _ in app.applyKnobs() }
        .onChange(of: app.fill) { _, _ in app.applyKnobs() }
        .onChange(of: app.theme) { _, _ in app.applyKnobs() }
        .background(ShortcutCatcher(app: app))
        .sheet(isPresented: $app.newProjectOpen) {
            NewProjectView(isLanding: false)
                .frame(width: Tokens.Window.newProjectWidth, height: Tokens.Window.newProjectHeight)
                .environmentObject(app)
                .themeColors(colors)
                .l10n(l10n)
        }
    }

    private var canvasIsland: some View {
        let shape = RoundedRectangle(cornerRadius: Tokens.Radius.island, style: .continuous)
        return BoardCanvas()
            .clipShape(shape)
            .background(colors.surface, in: shape)
            .overlay(shape.strokeBorder(colors.islandBorder, lineWidth: 1))
            .overlay {
                if artworkDropTargeted {
                    shape.fill(colors.accentTeal.opacity(0.15))
                    shape.strokeBorder(colors.accentTeal, lineWidth: 2)
                }
            }
            .overlay(alignment: .bottomTrailing) {
                zoomControls
                    .padding(Tokens.Space.md)
            }
            .onDrop(of: ArtworkImport.dropTypes, isTargeted: $artworkDropTargeted) { providers in
                app.dropArtworkIntoWorkspace(providers: providers)
            }
    }

    private var zoomControls: some View {
        CalmIsland(padding: Tokens.Space.sm) {
            HStack(spacing: Tokens.Space.sm) {
                zoomStepButton(zoomIn: false, label: "−")
                Slider(
                    value: Binding(
                        get: { Double(app.engine.state.zoomUnit) },
                        set: { app.engine.setZoomUnit(Float($0)) }
                    ),
                    in: 0...1
                )
                .controlSize(.mini)
                .frame(width: 96)
                zoomStepButton(zoomIn: true, label: "+")
                CalmText.muted("\(Int(app.engine.state.zoom * 100))%", mono: true)
                    .frame(width: 40, alignment: .trailing)
                Button {
                    app.engine.fit()
                } label: {
                    AppIcon.fitToView(color: colors.accentTeal)
                }
                .buttonStyle(.plain)
                .help(l10n.fitToView)
                .calmPointer()
            }
        }
        .fixedSize()
    }

    private func zoomStepButton(zoomIn: Bool, label: String) -> some View {
        Button {
            app.engine.stepZoom(in: zoomIn)
        } label: {
            Text(label)
                .font(.system(size: Tokens.TypeSize.title, weight: .medium))
                .foregroundStyle(colors.textMuted)
                .frame(width: 18, height: 18)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .calmPointer()
    }

    @ToolbarContentBuilder
    private var editorToolbar: some ToolbarContent {
        ToolbarItemGroup(placement: .navigation) {
            ForEach(app.openTabs) { tab in
                CalmChip(
                    title: tab.name,
                    accent: tab.accentColor,
                    selected: app.activeTabId == tab.id,
                    onAccent: { editingTab = tab.id },
                    onSelect: { app.switchTo(projectId: tab.id, info: tab) },
                    onClose: { app.closeTab(tab.id) }
                )
                .popover(
                    isPresented: Binding(
                        get: { editingTab == tab.id },
                        set: { if !$0 { editingTab = nil } }
                    ),
                    arrowEdge: .bottom
                ) {
                    ProjectSettingsCard(project: tab)
                        .environmentObject(app)
                        .themeColors(colors)
                        .l10n(l10n)
                }
            }
            CalmIconButton {
                app.newProjectOpen = true
            } icon: {
                AppIcon.plus(color: colors.textMuted, size: 24)
            }
            .help(l10n.newProject)
            .calmPointer()
        }
    }

    private var layersIsland: some View {
        CalmIsland {
            VStack(alignment: .leading, spacing: Tokens.Space.md) {
                HStack {
                    CalmText.label(l10n.layers)
                    Spacer()
                    Button {
                        app.engine.addLayer()
                    } label: {
                        AppIcon.plus(color: colors.textMuted)
                    }
                    .buttonStyle(.plain)
                    .calmPointer()
                }
                ForEach((0..<app.engine.layerNames.count).reversed(), id: \.self) { index in
                    layerRow(index)
                }
                Spacer(minLength: 0)
                canvasResizeFooter
            }
            .frame(maxHeight: .infinity, alignment: .top)
        }
        .frame(width: 252)
        .overlay(alignment: .leading) {
            if let hoveredLayer {
                let name = app.engine.layerNames[hoveredLayer]
                LayerHoverCard(name: name, image: app.engine.layerThumbnail(index: hoveredLayer))
                    .offset(x: -(192 + Tokens.Space.md))
                    .transition(.opacity)
            }
        }
        .onAppear { syncCanvasSize() }
        .onChange(of: app.engine.state.width) { _, _ in syncCanvasSize() }
        .onChange(of: app.engine.state.height) { _, _ in syncCanvasSize() }
    }

    private func syncCanvasSize() {
        canvasWidth = Int(app.engine.state.width)
        canvasHeight = Int(app.engine.state.height)
    }

    private var canvasResizeFooter: some View {
        HStack(spacing: Tokens.Space.sm) {
            CalmText.label(l10n.canvasWidth)
            CalmNumberField(value: $canvasWidth, width: 56)
                .onSubmit { app.engine.resizeDocument(width: canvasWidth, height: canvasHeight) }
            CalmText.label(l10n.canvasHeight)
            CalmNumberField(value: $canvasHeight, width: 56)
                .onSubmit { app.engine.resizeDocument(width: canvasWidth, height: canvasHeight) }
        }
    }

    private func layerRow(_ index: Int) -> some View {
        let selected = app.engine.state.activeLayer == UInt32(index)
        let name = app.engine.layerNames[index]
        let visible = index < app.engine.layerVisibles.count ? app.engine.layerVisibles[index] : true
        return HStack(spacing: Tokens.Space.sm) {
            Button {
                app.engine.setLayerVisible(index, visible: !visible)
            } label: {
                AppIcon.eye(color: visible ? colors.textMuted : colors.textMuted.opacity(0.45), open: visible)
            }
            .buttonStyle(.plain)
            .calmPointer()

            Button {
                app.engine.setActiveLayer(index)
            } label: {
                HStack(spacing: Tokens.Space.md) {
                    layerThumb(index)
                    CalmText.body(name, strong: selected)
                        .opacity(visible ? 1 : 0.45)
                    Spacer()
                }
                .padding(Tokens.Space.sm)
                .calmSurface(hover: selected, radius: Tokens.Radius.sm)
            }
            .buttonStyle(.plain)
            .calmPointer()

            Button {
                layerSettingsIndex = index
            } label: {
                AppIcon.more(color: colors.textMuted)
            }
            .buttonStyle(.plain)
            .calmPointer()
            .popover(
                isPresented: Binding(
                    get: { layerSettingsIndex == index },
                    set: { if !$0 { layerSettingsIndex = nil } }
                ),
                arrowEdge: .bottom
            ) {
                LayerSettingsCard(index: index, canMergeDown: index > 0 && app.engine.layerNames[index - 1] != l10n.paper)
                    .environmentObject(app)
                    .themeColors(colors)
                    .l10n(l10n)
            }

            Button {
                app.engine.removeLayer(index)
                if hoveredLayer == index {
                    hoveredLayer = nil
                }
            } label: {
                AppIcon.trash(color: colors.textMuted)
            }
            .buttonStyle(.plain)
            .calmPointer()
        }
        .onHover { hovering in
            app.engine.setHoverLayer(hovering ? index : nil)
            hoveredLayer = hovering ? index : nil
        }
    }

    private func layerThumb(_ index: Int) -> some View {
        let shape = RoundedRectangle(cornerRadius: Tokens.Radius.sm, style: .continuous)
        return ZStack {
            shape.fill(colors.surfaceHover)
            if let image = app.engine.layerThumbnail(index: index, maxSide: 96) {
                Image(nsImage: image)
                    .resizable()
                    .aspectRatio(contentMode: .fill)
                    .frame(width: 40, height: 28)
                    .clipShape(shape)
            } else {
                Text("\(index + 1)")
                    .font(.system(size: Tokens.TypeSize.label, weight: .bold))
                    .foregroundStyle(colors.textMuted)
            }
        }
        .frame(width: 40, height: 28)
    }

    private func toolButton<Icon: View>(_ tool: CalmTool, @ViewBuilder icon: () -> Icon) -> some View {
        CalmToolButton(selected: app.tool == tool, action: { app.selectTool(tool) }) { icon() }
    }

    private func iconColor(_ tool: CalmTool) -> Color {
        app.tool == tool ? colors.accentTeal : colors.textMuted
    }
}

private struct LayerHoverCard: View {
    @Environment(\.themeColors) private var colors
    let name: String
    let image: NSImage?

    var body: some View {
        VStack(alignment: .leading, spacing: Tokens.Space.sm) {
            CalmText.body(name, strong: true)
            if let image {
                Image(nsImage: image)
                    .resizable()
                    .interpolation(.high)
                    .aspectRatio(contentMode: .fit)
                    .frame(width: 160, height: 120)
                    .background(colors.surfaceHover)
                    .clipShape(RoundedRectangle(cornerRadius: Tokens.Radius.sm, style: .continuous))
            } else {
                CalmThumb(width: 160, height: 120)
            }
        }
        .padding(Tokens.Space.md)
        .frame(width: 192)
        .background(
            colors.surface,
            in: RoundedRectangle(cornerRadius: Tokens.Radius.sm, style: .continuous)
        )
    }
}
