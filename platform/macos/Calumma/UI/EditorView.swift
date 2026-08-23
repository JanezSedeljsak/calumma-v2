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
    @State private var boundsX = 0
    @State private var boundsY = 0
    @State private var boundsWidth = 0
    @State private var boundsHeight = 0
    @State private var layerSettingsIndex: Int?
    @State private var renamingLayer: Int?
    @State private var renameDraft = ""
    @State private var dropTargetRow: Int?
    @State private var draggingRow: Int?
    @State private var aiBlinkOn = false

    var body: some View {
        HStack(alignment: .top, spacing: Tokens.Space.sm) {
            ToolsPanel()
                .frame(maxHeight: .infinity)
                .zIndex(2)

            canvasIsland
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .zIndex(0)

            if app.layersOpen {
                layersIsland
                    .frame(maxHeight: .infinity)
                    .zIndex(1)
            }
        }
        .padding([.horizontal, .bottom], Tokens.Space.sm)
        .padding(.top, Tokens.Space.xs)
        .calmScreen()
        .toolbar { editorToolbar }
        .onAppear { app.applyKnobs() }
        .onChange(of: app.tool) { _, _ in
            app.applyKnobs()
        }
        .onChange(of: app.color) { _, _ in app.applyKnobs() }
        .onChange(of: app.brushSize) { _, _ in app.applyKnobs() }
        .onChange(of: app.inkOpacity) { _, _ in app.applyKnobs() }
        .onChange(of: app.blurStrength) { _, _ in app.applyKnobs() }
        .onChange(of: app.tolerance) { _, _ in app.applyKnobs() }
        .onChange(of: app.brush) { _, _ in app.applyKnobs() }
        .onChange(of: app.eraserHardness) { _, _ in app.applyKnobs() }
        .onChange(of: app.fill) { _, _ in app.applyKnobs() }
        .onChange(of: app.vectorMode) { _, _ in app.applyKnobs() }
        .onChange(of: app.theme) { _, _ in app.applyKnobs() }
        .background(ShortcutCatcher(app: app))
        .sheet(isPresented: $app.newProjectOpen) {
            NewProjectView(isLanding: false)
                .frame(width: Tokens.Window.newProjectWidth, height: Tokens.Window.newProjectHeight)
                .environmentObject(app)
                .themeColors(colors)
                .l10n(l10n)
        }
        .calmToast(app.toast)
        .onChange(of: app.engine.aiOpBusyLayer, initial: true) { _, busyLayer in
            if busyLayer != nil {
                withAnimation(.easeInOut(duration: 0.55).repeatForever(autoreverses: true)) {
                    aiBlinkOn = true
                }
            } else {
                aiBlinkOn = false
            }
        }
    }

    private var canvasIsland: some View {
        let shape = RoundedRectangle(cornerRadius: Tokens.Radius.island, style: .continuous)
        return canvasWithRulers
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

    /// Horizontal ruler along the top, vertical along the left, both inset (Figma-style) so
    /// ticks never sit over paint — the board's own viewport just shrinks to what's left, the
    /// same way it already reacts to any other resize (`BoardCanvas.Coordinator.resize`).
    private var canvasWithRulers: some View {
        VStack(spacing: 0) {
            HStack(spacing: 0) {
                rulerCorner
                horizontalRuler
            }
            .frame(height: RulerView.thickness)

            HStack(spacing: 0) {
                verticalRuler
                    .frame(width: RulerView.thickness)
                BoardCanvas()
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .overlay(alignment: .topLeading) {
                        eyedropperLoupeOverlay
                    }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }

    private var rulerCorner: some View {
        Rectangle()
            .fill(colors.surface)
            .frame(width: RulerView.thickness, height: RulerView.thickness)
    }

    private var horizontalRuler: some View {
        RulerView(
            axis: .horizontal,
            ticks: app.engine.rulerTicksX(),
            zoom: app.engine.state.zoom,
            pan: app.engine.state.panX,
            engine: app.engine
        )
        .overlay(alignment: .bottom) {
            Rectangle().fill(colors.islandBorder).frame(height: 1)
        }
    }

    private var verticalRuler: some View {
        RulerView(
            axis: .vertical,
            ticks: app.engine.rulerTicksY(),
            zoom: app.engine.state.zoom,
            pan: app.engine.state.panY,
            engine: app.engine
        )
        .overlay(alignment: .trailing) {
            Rectangle().fill(colors.islandBorder).frame(width: 1)
        }
    }

    @ViewBuilder
    private var eyedropperLoupeOverlay: some View {
        if app.tool == .eyedropper, let loupe = app.eyedropperLoupe {
            HStack(spacing: Tokens.Space.xs) {
                Circle()
                    .fill(loupe.color)
                    .frame(width: 18, height: 18)
                    .overlay(
                        Circle().strokeBorder(colors.islandBorder, lineWidth: 1)
                    )
                CalmText.muted("#\(loupe.hex)", mono: true)
            }
            .padding(.horizontal, Tokens.Space.sm)
            .padding(.vertical, Tokens.Space.xs)
            .background(
                colors.surfaceHover,
                in: RoundedRectangle(cornerRadius: Tokens.Radius.sm, style: .continuous)
            )
            .overlay(
                RoundedRectangle(cornerRadius: Tokens.Radius.sm, style: .continuous)
                    .strokeBorder(colors.islandBorder, lineWidth: 1)
            )
            .offset(x: loupe.x + Tokens.Space.md, y: loupe.y + Tokens.Space.md)
            .allowsHitTesting(false)
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
                .help(l10n.zoom)
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
        .help(zoomIn ? l10n.zoomIn : l10n.zoomOut)
        .calmPointer()
    }

    @ToolbarContentBuilder
    private var editorToolbar: some ToolbarContent {
        ToolbarItem(placement: .navigation) {
            WorkspaceTitlebarTabs(editingTab: $editingTab)
        }
    }

    /// How much of the layers island the scrolling stack may claim before it starts scrolling
    /// instead of growing. The rest is reserved for the header and the bounds/canvas fields,
    /// which must stay reachable no matter how deep the stack gets.
    private static let layerListHeightFraction: CGFloat = 0.5

    private var layersIsland: some View {
        CalmIsland {
            GeometryReader { proxy in
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
                        .calmTooltip(l10n.addLayer, edge: .leading)
                        .calmPointer()
                    }
                    // The stack scrolls; the header above and the bounds/canvas fields below
                    // stay put. Capped against the island's own height rather than a constant
                    // so the list keeps its share of a resized window.
                    ScrollView(.vertical) {
                        VStack(alignment: .leading, spacing: Tokens.Space.md) {
                            ForEach((0..<app.engine.layerNames.count).reversed(), id: \.self) {
                                index in
                                layerRow(index)
                            }
                        }
                        .calmScrollBars()
                    }
                    .frame(maxHeight: proxy.size.height * Self.layerListHeightFraction)
                    .onDrop(of: [.text], isTargeted: nil) { _ in
                        draggingRow = nil
                        dropTargetRow = nil
                        return true
                    }
                    CalmDivider()
                    Spacer(minLength: 0)
                    CalmDivider()
                    panelFooter
                }
                .frame(maxHeight: .infinity, alignment: .top)
            }
        }
        .frame(width: 276)
        .overlay(alignment: .leading) {
            if let hoveredLayer {
                let name = app.engine.layerNames[hoveredLayer]
                LayerHoverCard(name: name, image: app.engine.layerPreviewCard(index: hoveredLayer))
                    .offset(x: -(192 + Tokens.Space.md))
                    .transition(.opacity)
            }
        }
        .onAppear {
            syncCanvasSize()
            syncLayerBounds()
        }
        .onChange(of: app.engine.state.width) { _, _ in syncCanvasSize() }
        .onChange(of: app.engine.state.height) { _, _ in syncCanvasSize() }
        .onChange(of: app.engine.state.activeLayer) { _, _ in syncLayerBounds() }
        .onChange(of: app.engine.layerNames.count) { _, _ in syncLayerBounds() }
        .onChange(of: app.engine.thumbnailRevision) { _, _ in syncLayerBounds() }
    }

    private func syncCanvasSize() {
        canvasWidth = Int(app.engine.state.width)
        canvasHeight = Int(app.engine.state.height)
    }

    /// Mirrors the engine's answer, never the last thing typed — a size larger than the layer
    /// is clamped rather than scaling it up, so the field has to show what actually landed.
    private func syncLayerBounds() {
        guard let bounds = app.engine.layerBounds(index: Int(app.engine.state.activeLayer)) else {
            boundsX = 0
            boundsY = 0
            boundsWidth = 0
            boundsHeight = 0
            return
        }
        boundsX = Int(bounds.x.rounded())
        boundsY = Int(bounds.y.rounded())
        boundsWidth = Int(bounds.width.rounded())
        boundsHeight = Int(bounds.height.rounded())
    }

    private func commitLayerBounds() {
        app.engine.setLayerBounds(
            index: Int(app.engine.state.activeLayer),
            x: Float(boundsX),
            y: Float(boundsY),
            width: Float(boundsWidth),
            height: Float(boundsHeight)
        )
        syncLayerBounds()
    }

    /// Layer bounds and canvas size share one `Grid` rather than sitting in separate stacks, so
    /// all six fields keep the same two columns — label widths differ per glyph ("W" is wider
    /// than "X"), which is what pushed the fields out of line when each row measured itself.
    private var panelFooter: some View {
        Grid(
            alignment: .leading,
            horizontalSpacing: Tokens.Space.sm,
            verticalSpacing: Tokens.Space.sm
        ) {
            GridRow {
                CalmText.label(l10n.layerBounds).gridCellColumns(4)
            }
            GridRow {
                CalmText.label(l10n.layerBoundsX)
                CalmNumberField(value: $boundsX, width: 56)
                    .onSubmit { commitLayerBounds() }
                CalmText.label(l10n.layerBoundsY)
                CalmNumberField(value: $boundsY, width: 56)
                    .onSubmit { commitLayerBounds() }
            }
            GridRow {
                CalmText.label(l10n.canvasWidth)
                CalmNumberField(value: $boundsWidth, width: 56)
                    .onSubmit { commitLayerBounds() }
                CalmText.label(l10n.canvasHeight)
                CalmNumberField(value: $boundsHeight, width: 56)
                    .onSubmit { commitLayerBounds() }
            }
            CalmDivider().gridCellColumns(4)
            GridRow {
                CalmText.label(l10n.canvasWidth)
                CalmNumberField(value: $canvasWidth, width: 56)
                    .onSubmit { app.engine.resizeDocument(width: canvasWidth, height: canvasHeight) }
                CalmText.label(l10n.canvasHeight)
                CalmNumberField(value: $canvasHeight, width: 56)
                    .onSubmit { app.engine.resizeDocument(width: canvasWidth, height: canvasHeight) }
            }
        }
    }

    /// Panel rows run top-first while the engine stores the stack bottom-first. The shell only
    /// ever names rows — `calm_engine_move_layer_row` owns the flip.
    private func layerDisplayRow(_ index: Int) -> Int {
        app.engine.layerNames.count - 1 - index
    }

    private func beginRename(_ index: Int) {
        renameDraft = app.engine.layerNames[index]
        renamingLayer = index
    }

    private func commitRename(_ index: Int) {
        renamingLayer = nil
        app.engine.setLayerName(index, name: renameDraft)
    }

    private func layerRow(_ index: Int) -> some View {
        let selected = app.engine.state.activeLayer == UInt32(index)
        let name = app.engine.layerNames[index]
        let visible = index < app.engine.layerVisibles.count ? app.engine.layerVisibles[index] : true
        let locked = index < app.engine.layerLocked.count ? app.engine.layerLocked[index] : false
        let isPaper = app.engine.isLayerPaper(index: index)
        let renameable = !isPaper
        let row = layerDisplayRow(index)
        return HStack(spacing: Tokens.Space.sm) {
            Button {
                app.engine.setLayerVisible(index, visible: !visible)
            } label: {
                AppIcon.eye(color: visible ? colors.textMuted : colors.textMuted.opacity(0.45), open: visible)
            }
            .buttonStyle(.plain)
            .calmTooltip(l10n.layerVisibility, edge: .leading)
            .calmPointer()

            Button {
                app.engine.setLayerLocked(index, locked: !locked)
            } label: {
                AppIcon.lock(
                    color: locked ? colors.accentTeal : colors.textMuted.opacity(0.45),
                    closed: locked
                )
            }
            .buttonStyle(.plain)
            .calmTooltip(locked ? l10n.layerUnlock : l10n.layerLock, edge: .leading)
            .calmPointer()

            Button {
                app.engine.setActiveLayer(index)
            } label: {
                HStack(spacing: Tokens.Space.md) {
                    layerThumb(index)
                    if renamingLayer == index {
                        CalmField(text: $renameDraft)
                            .onSubmit { commitRename(index) }
                            .onExitCommand { renamingLayer = nil }
                    } else {
                        CalmText.body(name, strong: selected)
                            .lineLimit(1)
                            .truncationMode(.tail)
                            .opacity(visible ? 1 : 0.45)
                            .calmTooltip(name, edge: .trailing)
                    }
                    Spacer()
                    if app.engine.isLayerVector(index: index) {
                        CalmText.muted("\(app.engine.layerItemCount(index: index))", mono: true)
                    }
                }
                .padding(Tokens.Space.sm)
                .calmSurface(
                    hover: selected || dropTargetRow == row,
                    radius: Tokens.Radius.sm,
                    bordered: true,
                    focused: selected
                )
            }
            .buttonStyle(.plain)
            .calmPointer()
            .simultaneousGesture(
                TapGesture(count: 2).onEnded {
                    if app.engine.isLayerText(index: index) {
                        app.selectTool(.text)
                        app.engine.editTextLayer(index)
                    } else if renameable {
                        beginRename(index)
                    }
                }
            )
            .contextMenu {
                if app.engine.isLayerText(index: index) {
                    Button(l10n.editText) {
                        app.selectTool(.text)
                        app.engine.editTextLayer(index)
                    }
                }
                if renameable {
                    Button(l10n.renameLayer) { beginRename(index) }
                }
            }

            Button {
                layerSettingsIndex = index
            } label: {
                AppIcon.more(color: colors.textMuted)
            }
            .buttonStyle(.plain)
            .calmTooltip(l10n.layerSettings, edge: .leading)
            .calmPointer()
            .popover(
                isPresented: Binding(
                    get: { layerSettingsIndex == index },
                    set: { if !$0 { layerSettingsIndex = nil } }
                ),
                arrowEdge: .bottom
            ) {
                LayerSettingsCard(
                    index: index,
                    canMoveUp: index < app.engine.layerNames.count - 1 && !isPaper,
                    canMoveDown: index > 0
                        && !isPaper
                        && !(index == 1 && app.engine.isLayerPaper(index: 0)),
                    canMergeDown: index > 0 && !app.engine.isLayerPaper(index: index - 1)
                )
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
            .calmTooltip(l10n.deleteLayer, edge: .leading)
            .calmPointer()
        }
        .onHover { hovering in
            app.engine.setHoverLayer(hovering ? index : nil)
            hoveredLayer = hovering ? index : nil
        }
        .opacity(draggingRow == row ? 0.4 : 1)
        .onDrag {
            draggingRow = row
            return NSItemProvider(object: String(row) as NSString)
        }
        .onDrop(
            of: [.text],
            delegate: LayerDropDelegate(
                row: row,
                target: $dropTargetRow,
                dragging: $draggingRow,
                move: { from, to in app.engine.moveLayerRow(from: from, to: to) }
            )
        )
    }

    /// Dropping *onto* a row means "take the row you are dragging and put it here", which is a
    /// simpler contract than an insertion point between rows: there is no off-by-one to get
    /// wrong at either end, and every drop lands somewhere legal or is refused by the engine.
    private struct LayerDropDelegate: DropDelegate {
        let row: Int
        @Binding var target: Int?
        @Binding var dragging: Int?
        let move: (Int, Int) -> Void

        func dropEntered(info: DropInfo) {
            target = row
        }

        func dropExited(info: DropInfo) {
            if target == row {
                target = nil
            }
        }

        func dropUpdated(info: DropInfo) -> DropProposal? {
            DropProposal(operation: .move)
        }

        func performDrop(info: DropInfo) -> Bool {
            target = nil
            guard let from = dragging else { return false }
            dragging = nil
            guard from != row else { return false }
            move(from, row)
            return true
        }
    }

    private func layerThumb(_ index: Int) -> some View {
        let shape = RoundedRectangle(cornerRadius: Tokens.Radius.sm, style: .continuous)
        let busy = app.engine.aiOpBusyLayer == index
        return ZStack {
            shape.fill(colors.surfaceHover)
            if let image = app.engine.layerThumbnail(index: index) {
                // Already cropped and sized to exactly this frame by `rowThumbnail`, so it
                // draws 1:1 — no `.resizable()`, no aspect fit to recompute, nothing for
                // AppKit to resample on every re-render of the panel.
                Image(nsImage: image)
                    .clipShape(shape)
            } else {
                Text("\(index + 1)")
                    .font(.system(size: Tokens.TypeSize.label, weight: .bold))
                    .foregroundStyle(colors.textMuted)
            }
        }
        .frame(width: 40, height: 28)
        // An AI op running against this layer blinks its thumbnail — the only feedback a
        // longer-running op like Remove Background had before was silence either way, so
        // this is the difference between "still working" and "looks broken."
        .overlay(
            shape.strokeBorder(colors.accentTeal, lineWidth: busy ? 1.5 : 0)
        )
        .opacity(busy && aiBlinkOn ? 0.35 : 1)
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
