import AppKit
import SwiftUI

struct EditorView: View {
    @EnvironmentObject private var app: AppModel
    @Environment(\.themeColors) private var colors
    @Environment(\.l10n) private var l10n
    @State private var shapePickerOpen = false
    @State private var hoveredLayer: Int?

    var body: some View {
        ZStack {
            BoardCanvas()
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            HStack(alignment: .top, spacing: 0) {
                toolIsland
                    .padding(.leading, Tokens.Space.lg)
                    .padding(.top, Tokens.Space.lg)
                Spacer(minLength: 0)
                if app.layersOpen {
                    layersIsland
                        .padding(.trailing, Tokens.Space.lg)
                        .padding(.top, Tokens.Space.lg)
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        }
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
    }

    @ToolbarContentBuilder
    private var editorToolbar: some ToolbarContent {
        ToolbarItemGroup(placement: .navigation) {
            ForEach(app.openTabs) { tab in
                CalmChip(
                    title: tab.name,
                    selected: app.activeTabId == tab.id,
                    onSelect: { app.switchTo(projectId: tab.id, info: tab) },
                    onClose: { app.closeTab(tab.id) }
                )
            }
            Button {
                app.showNewProject()
            } label: {
                AppIcon.plus(color: colors.textMuted)
            }
            .buttonStyle(.plain)
            .help(l10n.newProject)
            .calmPointer()
        }
        ToolbarItem(placement: .automatic) {
            Button {
                app.settingsOpen = true
            } label: {
                AppIcon.settings(color: colors.textMuted)
            }
            .buttonStyle(.plain)
            .help(l10n.settings)
            .calmPointer()
        }
    }

    private var toolIsland: some View {
        CalmIsland {
            VStack(spacing: Tokens.Space.md) {
                toolButton(.pen) { AppIcon.pen(color: iconColor(.pen)) }

                shapeToolButton

                ColorPicker("", selection: $app.color, supportsOpacity: true)
                    .labelsHidden()
                    .frame(width: 28, height: 28)
                    .clipShape(RoundedRectangle(cornerRadius: Tokens.Radius.sm, style: .continuous))

                VStack(spacing: Tokens.Space.xs) {
                    CalmText.muted("\(Int(app.brushSize))", mono: true)
                    Slider(value: Binding(
                        get: { Double(app.brushSize) },
                        set: { app.brushSize = Float($0) }
                    ), in: 1...96)
                    .frame(width: 72)
                }

                if app.tool.isShape {
                    Toggle(isOn: $app.fill) {
                        CalmText.muted(l10n.fill)
                    }
                    .toggleStyle(.switch)
                    .controlSize(.mini)
                }

                zoomControls

                Spacer(minLength: Tokens.Space.md)

                if app.engine.canRemoveBackground {
                    CalmPlainButton(title: l10n.cutBackground, accent: true) {
                        app.engine.removeBackground()
                    }
                }

                CalmPlainButton(title: l10n.undo, enabled: app.engine.state.canUndo) {
                    app.engine.undo()
                }
                CalmPlainButton(title: l10n.redo, enabled: app.engine.state.canRedo) {
                    app.engine.redo()
                }
                CalmPlainButton(
                    title: app.theme == .dark ? l10n.themeLight : l10n.themeDark,
                    accent: true
                ) {
                    app.toggleTheme()
                }
            }
        }
        .frame(width: 96)
    }

    private var shapeToolButton: some View {
        let active = app.tool.isShape
        let current = active ? app.tool : app.lastShapeTool
        return CalmToolButton(selected: active, action: {
            if active {
                shapePickerOpen.toggle()
            } else {
                app.selectTool(app.lastShapeTool)
                shapePickerOpen = true
            }
        }) {
            shapeIcon(current, color: active ? colors.accentTeal : colors.textMuted)
        }
        .popover(isPresented: $shapePickerOpen, arrowEdge: .trailing) {
            HStack(spacing: Tokens.Space.sm) {
                shapePick(.line)
                shapePick(.rect)
                shapePick(.ellipse)
                shapePick(.arrow)
            }
            .padding(Tokens.Space.sm)
            .background(colors.surface)
        }
        .help(l10n.shapes)
    }

    private func shapePick(_ tool: CalmTool) -> some View {
        CalmToolButton(selected: app.tool == tool, action: {
            app.selectTool(tool)
            shapePickerOpen = false
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
        }
    }

    private var zoomControls: some View {
        let minZ = Double(max(app.engine.state.minZoom, Float.leastNormalMagnitude))
        let maxZ = Double(max(app.engine.state.maxZoom, app.engine.state.minZoom))
        return VStack(spacing: Tokens.Space.xs) {
            CalmText.muted(l10n.zoom)
            CalmText.muted("\(Int(app.engine.state.zoom * 100))%", mono: true)
            Slider(
                value: Binding(
                    get: {
                        logZoomUnit(zoom: Double(app.engine.state.zoom), minZoom: minZ, maxZoom: maxZ)
                    },
                    set: { unit in
                        app.engine.setZoom(Float(zoomFromLogUnit(unit, minZoom: minZ, maxZoom: maxZ)))
                    }
                ),
                in: 0...1
            )
            .frame(width: 72)
            Button(l10n.fitToView) {
                app.engine.fit()
            }
            .buttonStyle(.plain)
            .font(.system(size: Tokens.TypeSize.label))
            .foregroundStyle(colors.textMuted)
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
                ForEach(0..<app.engine.layerNames.count, id: \.self) { index in
                    layerRow(index)
                }
                Spacer(minLength: 0)
            }
        }
        .frame(width: 220)
        .overlay(alignment: .leading) {
            if let hoveredLayer {
                let name = app.engine.layerNames[hoveredLayer]
                LayerHoverCard(name: name, image: app.engine.layerThumbnail(index: hoveredLayer))
                    .offset(x: -(192 + Tokens.Space.md))
                    .transition(.opacity)
            }
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
                    CalmThumb(width: 40, height: 28, label: "\(index + 1)")
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

    private func toolButton<Icon: View>(_ tool: CalmTool, @ViewBuilder icon: () -> Icon) -> some View {
        CalmToolButton(selected: app.tool == tool, action: { app.selectTool(tool) }) { icon() }
    }

    private func iconColor(_ tool: CalmTool) -> Color {
        app.tool == tool ? colors.accentTeal : colors.textMuted
    }
}

private func logZoomUnit(zoom: Double, minZoom: Double, maxZoom: Double) -> Double {
    guard maxZoom > minZoom, minZoom > 0, zoom > 0 else { return 0 }
    let t = log(zoom / minZoom) / log(maxZoom / minZoom)
    return min(1, max(0, t))
}

private func zoomFromLogUnit(_ unit: Double, minZoom: Double, maxZoom: Double) -> Double {
    guard maxZoom > minZoom, minZoom > 0 else { return minZoom }
    let t = min(1, max(0, unit))
    return minZoom * pow(maxZoom / minZoom, t)
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
        weak var app: AppModel?

        override var acceptsFirstResponder: Bool { true }

        override func keyDown(with event: NSEvent) {
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
                app.engine.clearLayer()
                return
            }
            if flags.contains(.command), chars == "=" || chars == "+" {
                app.engine.zoom(x: 400, y: 300, factor: 1.25)
                return
            }
            if flags.contains(.command), chars == "-" {
                app.engine.zoom(x: 400, y: 300, factor: 0.8)
                return
            }
            if flags.contains(.command), chars.lowercased() == "s" {
                app.engine.save()
                return
            }
            if flags.contains(.command), flags.contains(.option), chars.lowercased() == "l" {
                app.layersOpen.toggle()
                return
            }
            if flags.contains(.command), chars.lowercased() == "t" {
                app.toggleTheme()
                return
            }
            switch chars.lowercased() {
            case "p": app.selectTool(.pen); return
            case "l": app.selectTool(.line); return
            case "r": app.selectTool(.rect); return
            case "o": app.selectTool(.ellipse); return
            case "a": app.selectTool(.arrow); return
            case "f": app.fill.toggle(); return
            case "0": app.engine.fit(); return
            case "[": app.brushSize = max(1, app.brushSize - 1); return
            case "]": app.brushSize = min(96, app.brushSize + 1); return
            default: break
            }
            super.keyDown(with: event)
        }

        override func keyUp(with event: NSEvent) {
            if event.keyCode == 49 {
                app?.spacePan = false
                NSCursor.crosshair.set()
                return
            }
            super.keyUp(with: event)
        }
    }
}
