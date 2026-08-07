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
            toolIsland
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
                AppIcon.plus(color: colors.textMuted)
            }
            .help(l10n.newProject)
            .calmPointer()
        }
    }

    private var toolIsland: some View {
        CalmIsland(padding: Tokens.Space.xs) {
            VStack(spacing: Tokens.Space.sm) {
                toolGrid
                toolOptions
                Spacer(minLength: Tokens.Space.xs)
                colorSection
                Spacer(minLength: Tokens.Space.xs)
                aiMenu
            }
            .frame(maxHeight: .infinity, alignment: .top)
        }
        .frame(width: 128)
    }

    private var toolGrid: some View {
        LazyVGrid(
            columns: [GridItem(.flexible()), GridItem(.flexible()), GridItem(.flexible())],
            spacing: Tokens.Space.xs
        ) {
            toolButton(.pen) { AppIcon.pen(color: iconColor(.pen)) }
            toolButton(.eraser) { AppIcon.eraser(color: iconColor(.eraser)) }
            shapeToolButton
            selectToolButton
            toolButton(.bucket) { AppIcon.bucket(color: iconColor(.bucket)) }
            toolButton(.transform) { AppIcon.transform(color: iconColor(.transform)) }
        }
    }

    @ViewBuilder
    private var toolOptions: some View {
        VStack(spacing: Tokens.Space.sm) {
            if app.tool.isShape {
                CalmText.label(l10n.shapes)
                HStack(spacing: Tokens.Space.xs) {
                    shapePick(.line)
                    shapePick(.rect)
                    shapePick(.ellipse)
                    shapePick(.arrow)
                }
            }

            if app.tool.isSelection {
                CalmText.label(l10n.selectionTools)
                HStack(spacing: Tokens.Space.xs) {
                    selectPick(.selectRect)
                    selectPick(.selectEllipse)
                    selectPick(.selectLasso)
                }
            }

            if !app.tool.isSelection, app.tool != .bucket, app.tool != .transform {
                VStack(spacing: Tokens.Space.xs) {
                    CalmText.muted("\(Int(app.brushSize))", mono: true)
                    Slider(value: Binding(
                        get: { Double(app.brushSize) },
                        set: { app.brushSize = Float($0) }
                    ), in: 1...96)
                    .controlSize(.mini)
                }
            }

            if app.tool.isShape {
                Toggle(isOn: $app.fill) {
                    CalmText.muted(l10n.fill)
                }
                .toggleStyle(.switch)
                .controlSize(.mini)
                .labelsHidden()
                .help(l10n.fill)
            }
        }
    }

    private var colorSection: some View {
        VStack(spacing: Tokens.Space.xs) {
            HStack(spacing: Tokens.Space.xs) {
                quickColorBox(0)
                quickColorBox(1)
            }
            ColorPicker("", selection: $app.color, supportsOpacity: true)
                .labelsHidden()
                .frame(height: 28)
                .clipShape(RoundedRectangle(cornerRadius: Tokens.Radius.sm, style: .continuous))
                .help(l10n.projectColor)
            hueSlider
        }
    }

    private func quickColorBox(_ index: Int) -> some View {
        let shape = RoundedRectangle(cornerRadius: Tokens.Radius.sm, style: .continuous)
        return Button {
            app.selectQuickColor(index)
        } label: {
            shape
                .fill(app.quickColors[index])
                .frame(maxWidth: .infinity)
                .frame(height: 24)
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

    private var hueSlider: some View {
        GeometryReader { geo in
            let knob: CGFloat = 10
            ZStack(alignment: .leading) {
                LinearGradient(colors: Self.hueSpectrum, startPoint: .leading, endPoint: .trailing)
                Circle()
                    .fill(.white)
                    .frame(width: knob, height: knob)
                    .overlay(Circle().strokeBorder(.black.opacity(0.3), lineWidth: 1))
                    .offset(x: CGFloat(app.colorHue) * max(geo.size.width - knob, 0))
            }
            .clipShape(RoundedRectangle(cornerRadius: Tokens.Radius.sm, style: .continuous))
            .contentShape(Rectangle())
            .gesture(
                DragGesture(minimumDistance: 0).onChanged { value in
                    let fraction = min(max(value.location.x / max(geo.size.width, 1), 0), 1)
                    app.setHue(Float(fraction))
                }
            )
        }
        .frame(height: 16)
    }

    private static let hueSpectrum: [Color] = stride(from: 0.0, through: 1.0, by: 1.0 / 11.0)
        .map { Color(hue: $0, saturation: 1, brightness: 1) }

    private var aiMenu: some View {
        Menu {
            Button(l10n.removeBackground) {
                app.engine.removeBackground()
            }
            .disabled(!app.engine.canRemoveBackground)
        } label: {
            AppIcon.ai(color: colors.textMuted)
                .frame(width: 32, height: 32)
                .contentShape(Rectangle())
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .help(l10n.ai)
        .calmPointer()
    }

    private var shapeToolButton: some View {
        let active = app.tool.isShape
        let current = active ? app.tool : app.lastShapeTool
        return CalmToolButton(selected: active, action: {
            app.selectTool(app.lastShapeTool)
        }) {
            shapeIcon(current, color: active ? colors.accentTeal : colors.textMuted)
        }
        .help(l10n.shapes)
    }

    private func shapePick(_ tool: CalmTool) -> some View {
        CalmToolButton(selected: app.tool == tool, action: {
            app.selectTool(tool)
        }) {
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
        case .pen: AppIcon.pen(color: color)
        case .eraser: AppIcon.eraser(color: color)
        case .bucket: AppIcon.bucket(color: color)
        case .transform: AppIcon.transform(color: color)
        case .selectRect, .selectEllipse, .selectLasso: AppIcon.selectRect(color: color)
        }
    }

    private var selectToolButton: some View {
        let active = app.tool.isSelection
        let current = active ? app.tool : app.lastSelectTool
        return CalmToolButton(selected: active, action: {
            app.selectTool(app.lastSelectTool)
        }) {
            selectionIcon(current, color: active ? colors.accentTeal : colors.textMuted)
        }
        .help(l10n.selectionTools)
    }

    private func selectPick(_ tool: CalmTool) -> some View {
        CalmToolButton(selected: app.tool == tool, action: {
            app.selectTool(tool)
        }) {
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

private struct ShortcutCatcher: NSViewRepresentable {
    let app: AppModel

    func makeNSView(context: Context) -> NSView {
        let view = KeyView()
        view.app = app
        DispatchQueue.main.async { view.window?.makeFirstResponder(view) }
        return view
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        (nsView as? KeyView)?.app = app
    }

    final class KeyView: NSView {
        nonisolated(unsafe) weak var app: AppModel?

        override var acceptsFirstResponder: Bool { true }

        override func keyDown(with event: NSEvent) {
            MainActor.assumeIsolated {
                handleKeyDown(event)
            }
        }

        override func keyUp(with event: NSEvent) {
            MainActor.assumeIsolated {
                if event.keyCode == 49 {
                    app?.spacePan = false
                    NSCursor.crosshair.set()
                    return
                }
                super.keyUp(with: event)
            }
        }

        @MainActor
        private func handleKeyDown(_ event: NSEvent) {
            if event.keyCode == 49 {
                if !event.isARepeat {
                    app?.spacePan = true
                    NSCursor.openHand.set()
                }
                return
            }
            let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
            let chars = event.charactersIgnoringModifiers ?? ""
            guard let app else {
                super.keyDown(with: event)
                return
            }
            if flags.contains(.command), flags.contains(.shift), chars.lowercased() == "z" {
                app.engine.redo()
                return
            }
            if flags.contains(.command), chars.lowercased() == "z" {
                app.engine.undo()
                return
            }
            if flags.contains(.command), flags.contains(.shift), chars.lowercased() == "n" {
                app.engine.addLayer()
                return
            }
            if flags.contains(.command), event.keyCode == 51 {
                if app.engine.hasSelection {
                    app.engine.clearSelectionPixels()
                } else {
                    app.engine.clearLayer()
                }
                return
            }
            if flags.contains(.command), chars.lowercased() == "c" {
                app.copySelectionOrCanvas()
                return
            }
            if flags.contains(.command), chars.lowercased() == "x" {
                app.cutSelection()
                return
            }
            if flags.contains(.command), chars.lowercased() == "v" {
                app.pasteFromClipboard()
                return
            }
            if event.keyCode == 53 {
                app.engine.deselect()
                return
            }
            if flags.contains(.command), chars == "=" || chars == "+" {
                app.engine.stepZoom(in: true)
                return
            }
            if flags.contains(.command), chars == "-" {
                app.engine.stepZoom(in: false)
                return
            }
            if flags.contains(.command), chars.lowercased() == "s" {
                app.engine.save()
                return
            }
            if flags.contains(.command), chars.lowercased() == "t" {
                app.selectTool(.transform)
                return
            }
            if flags.contains(.command), flags.contains(.option), chars.lowercased() == "l" {
                app.layersOpen.toggle()
                return
            }
            switch chars.lowercased() {
            case "p": app.selectTool(.pen); return
            case "l": app.selectTool(.line); return
            case "r": app.selectTool(.rect); return
            case "o": app.selectTool(.ellipse); return
            case "a": app.selectTool(.arrow); return
            case "e": app.selectTool(.eraser); return
            case "m": app.selectTool(app.lastSelectTool); return
            case "g": app.selectTool(.bucket); return
            case "f": app.fill.toggle(); return
            case "0": app.engine.fit(); return
            case "[": app.brushSize = max(1, app.brushSize - 1); return
            case "]": app.brushSize = min(96, app.brushSize + 1); return
            default: break
            }
            super.keyDown(with: event)
        }
    }
}
