import AppKit
import Combine
import SwiftUI
import UniformTypeIdentifiers

@MainActor
final class AppModel: ObservableObject {
    let engine: Engine
    @Published var theme: AppTheme = .dark
    @Published var language: AppLanguage = .en
    @Published private(set) var l10n: L10nCatalog = .load(.en)
    @Published var openWorkspaces: [WorkspaceInfo] = []
    @Published var activeWorkspaceId: String?
    @Published var activeProjectId: String?
    @Published var showLanding = true
    @Published var newProjectOpen = false
    @Published var settingsOpen = false
    @Published var workspaceExtendOpen = false
    @Published var artworkError: String?
    @Published private(set) var toast: ToastMessage?
    private var toastDismissWork: DispatchWorkItem?

    @Published var tool: CalmTool = .pen
    var lastShapeTool: CalmTool { engine.state.lastShapeTool }
    var lastSelectTool: CalmTool { engine.state.lastSelectTool }
    @Published var color: Color = Color(red: 0.1, green: 0.1, blue: 0.1) {
        didSet {
            quickColors[activeQuickColorIndex] = color
            if !editingHSB {
                hsb = HSBColor(color)
            }
        }
    }
    /// Three ink slots the picker switches between — primary, secondary, tertiary — and the
    /// only colors the tools panel ever shows. A shape reads two of them by *role* rather
    /// than by which is selected: primary outlines it, secondary fills it (`shapeStrokeColor`
    /// / `shapeFillColor`), which is why there is no fourth swatch for the outline any more.
    @Published var quickColors: [Color] = [
        Color(red: 0.1, green: 0.1, blue: 0.1),
        Color.white,
        Color(red: 0.5, green: 0.5, blue: 0.5),
    ]
    @Published var activeQuickColorIndex = 0
    @Published private(set) var hsb = HSBColor(Color(red: 0.1, green: 0.1, blue: 0.1))
    @Published private(set) var eyedropperLoupe: EyedropperLoupe?
    private var editingHSB = false
    @Published var brushSize: Float = Engine.brushSizeDefault
    @Published var eyedropperRadius: UInt32 = Engine.eyedropperRadiusDefault
    @Published var inkOpacity: Float = Engine.inkOpacityDefault
    @Published var blurStrength: Float = Engine.blurStrengthDefault
    @Published var tolerance: UInt8 = Engine.toleranceDefault
    @Published var brush: CalmBrush = .pen
    @Published var eraserHardness: Float = Engine.eraserHardnessDefault
    @Published var fill = false
    @Published var stroke = true
    @Published var vectorMode = false
    @Published var layersOpen = true
    @Published var spacePan = false

    weak var mainWindow: NSWindow? {
        didSet {
            mainWindow?.tabbingMode = .disallowed
        }
    }

    private var cancellables = Set<AnyCancellable>()

    var colors: ThemeColors { ThemeColors.colors(for: theme) }

    init() {
        engine = Engine()
        L10nStore.catalog = l10n
        engine.objectWillChange
            .receive(on: RunLoop.main)
            .sink { [weak self] in
                self?.objectWillChange.send()
            }
            .store(in: &cancellables)
        NotificationCenter.default
            .publisher(for: NSApplication.didResignActiveNotification)
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in
                MainActor.assumeIsolated { self?.endSpacePan() }
            }
            .store(in: &cancellables)
        restoreOpenWorkspaceTabs()
    }

    private func restoreOpenWorkspaceTabs() {
        let ids = engine.loadOpenWorkspaceTabs()
        guard !ids.isEmpty else { return }
        openWorkspaces = ids.compactMap { engine.workspace(id: $0) }
        if let first = openWorkspaces.first {
            switchToWorkspace(id: first.id)
        }
    }

    private func persistTabs() {
        engine.persistOpenWorkspaceTabs(openWorkspaces.map(\.id))
    }

    private func refreshOpenWorkspace(_ id: String) {
        guard let updated = engine.workspace(id: id) else { return }
        if let index = openWorkspaces.firstIndex(where: { $0.id == id }) {
            openWorkspaces[index] = updated
        }
    }

    func create(name: String, width: Int, height: Int, accent: Color? = nil) {
        let resolved = name.isEmpty ? l10n.untitled : name
        guard let id = engine.createProject(name: resolved, width: width, height: height) else {
            return
        }
        if let accent {
            engine.setAccent(projectId: id, color: accent)
        }
        adoptProject(id: id, name: resolved)
    }

    func copySelectionOrCanvas() {
        writePasteboard(engine.copy())
    }

    func copyLayer(index: Int) {
        writePasteboard(engine.copyLayer(index: index))
    }

    func cutSelection() {
        writePasteboard(engine.cut())
    }

    private func writePasteboard(_ payload: (Data, CalmClipboardKind)?) {
        guard let (data, kind) = payload else { return }
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        switch kind {
        case .svg:
            if let svg = String(data: data, encoding: .utf8) {
                pasteboard.setString(svg, forType: NSPasteboard.PasteboardType("public.svg-image"))
            }
        case .png:
            pasteboard.setData(data, forType: .png)
        }
    }

    func pasteFromClipboard() {
        guard let artwork = ArtworkImport.fromPasteboard() else { return }
        pasteIntoBoard(artwork)
    }

    /// A pasted image lands as a new layer at the top of the stack, and the first thing anyone
    /// does with one is put it somewhere — so the board hands it over already grabbed: Move
    /// selected, the new layer inside `⌘T` with its corners live. Dropping onto an open board
    /// is the same gesture by another route, so it goes through here too.
    private func pasteIntoBoard(_ artwork: ArtworkImage) {
        engine.pasteImage(
            premultipliedRGBA: artwork.premultipliedRGBA,
            width: artwork.width,
            height: artwork.height
        )
        enterMoveTransform()
    }

    /// Move is the transform tool: picking it puts the active layer inside `⌘T` so the same
    /// press that says "move this" hands back the handles that resize and rotate it.
    func enterMoveTransform() {
        selectTool(.move)
        engine.enterTransform()
    }

    func createFromArtwork(_ artwork: ArtworkImage?) {
        guard let artwork, let id = engine.createProject(name: artwork.name, artwork: artwork)
        else {
            artworkError = l10n.artworkImportFailed
            return
        }
        artworkError = nil
        adoptProject(id: id, name: artwork.name)
    }

    func importArtwork(url: URL) {
        createFromArtwork(ArtworkImport.decode(url: url))
    }

    func pasteArtwork() {
        createFromArtwork(ArtworkImport.fromPasteboard())
    }

    func chooseArtwork() {
        guard let url = ArtworkImport.chooseFile(
            prompt: l10n.chooseArtwork,
            message: l10n.pasteArtworkHint
        ) else {
            return
        }
        importArtwork(url: url)
    }

    func dropArtwork(providers: [NSItemProvider]) -> Bool {
        ArtworkImport.load(providers: providers) { [weak self] artwork in
            self?.createFromArtwork(artwork)
        }
    }

    func dropArtworkIntoWorkspace(providers: [NSItemProvider]) -> Bool {
        ArtworkImport.load(providers: providers) { [weak self] artwork in
            guard let artwork else { return }
            self?.pasteIntoBoard(artwork)
        }
    }

    func clearArtworkError() {
        artworkError = nil
    }

    func removeBackground() {
        engine.removeBackground { [weak self] result in
            guard let self else { return }
            switch result {
            case .success:
                showToast(l10n.removeBackgroundSuccess, kind: .success)
            case .failed:
                showToast(l10n.removeBackgroundFailed, kind: .error)
            case .ineligibleLayer:
                showToast(l10n.removeBackgroundNeedsRaster, kind: .error)
            }
        }
    }

    /// The one place a refused press interrupts. The engine has already thrown away every
    /// repeat of the same (layer, tool) question, so this fires when there is genuinely
    /// something new to say and never on the second try.
    func announceToolBlock() {
        guard let reason = engine.toolBlockNotice.reason(l10n) else { return }
        engine.clearToolBlockNotice()
        showToast(reason, kind: .error)
    }

    /// Shows `text` for a few seconds, then clears itself — unless a newer toast already
    /// replaced it, which comparing `toast?.id` against the one this call scheduled guards
    /// against.
    private func showToast(_ text: String, kind: ToastKind) {
        let message = ToastMessage(text: text, kind: kind)
        toast = message
        toastDismissWork?.cancel()
        let work = DispatchWorkItem { [weak self] in
            guard let self, toast?.id == message.id else { return }
            toast = nil
        }
        toastDismissWork = work
        DispatchQueue.main.asyncAfter(deadline: .now() + 3.2, execute: work)
    }

    private var activeProjectName: String {
        if let activeProjectId,
           let project = engine.recents.first(where: { $0.id == activeProjectId })
        {
            return project.name
        }
        return openWorkspaces.first { $0.id == activeWorkspaceId }?.name ?? l10n.untitled
    }

    func exportComposite(as format: ExportFormat) {
        guard let image = engine.compositeCGImage() else { return }
        let panel = NSSavePanel()
        panel.allowedContentTypes = [format.utType]
        panel.nameFieldStringValue = "\(activeProjectName).\(format.fileExtension)"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        guard let data = ImageEncode.data(image, format: format) else { return }
        try? data.write(to: url)
    }

    /// The document as one SVG file. Unlike the raster exports this one is *layered* — the
    /// engine keeps vector layers as geometry and embeds only the layers that really are
    /// pixels — so it is written from the engine's string, not from a composited image.
    func exportSVG() {
        guard let svg = engine.exportSVG(), let data = svg.data(using: .utf8) else { return }
        let panel = NSSavePanel()
        panel.allowedContentTypes = [.svg]
        panel.nameFieldStringValue = "\(activeProjectName).svg"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        try? data.write(to: url)
    }

    /// One layer to a file, the save-panel counterpart of `copyLayer`. A vector layer offers
    /// SVG first and keeps its geometry; anything else is written through the same raster
    /// encoders the whole-document export uses, so the format popup in the panel decides.
    func exportLayer(index: Int) {
        let isVector = engine.isLayerVector(index: index)
        let rasterTypes = ExportFormat.allCases.map(\.utType)
        let panel = NSSavePanel()
        panel.allowedContentTypes = isVector ? [.svg] + rasterTypes : rasterTypes
        panel.nameFieldStringValue = "\(activeProjectName)-\(layerFileName(index)).\(isVector ? "svg" : "png")"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        if url.pathExtension.lowercased() == "svg" {
            guard let svg = engine.layerSVG(index: index), let data = svg.data(using: .utf8)
            else {
                return
            }
            try? data.write(to: url)
            return
        }
        guard let format = ExportFormat(fileExtension: url.pathExtension),
              let image = engine.layerCGImage(index: index),
              let data = ImageEncode.data(image, format: format)
        else {
            return
        }
        try? data.write(to: url)
    }

    private func layerFileName(_ index: Int) -> String {
        let name = engine.layerNames.indices.contains(index) ? engine.layerNames[index] : ""
        let cleaned = name.components(separatedBy: CharacterSet(charactersIn: "/:\\")).joined(
            separator: "-")
        return cleaned.isEmpty ? l10n.formatKey("layerNamed", "\(index + 1)") : cleaned
    }

    func exportPSD() {
        guard let data = engine.exportPSD() else { return }
        let panel = NSSavePanel()
        panel.allowedContentTypes = [UTType(filenameExtension: "psd") ?? .data]
        panel.nameFieldStringValue = "\(activeProjectName).psd"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        try? data.write(to: url)
    }

    /// The document as one PDF. Layered like the SVG export rather than flattened like the
    /// raster ones, and written from the engine's bytes for the same reason.
    func exportPDF() {
        guard let data = engine.exportPDF() else { return }
        let panel = NSSavePanel()
        panel.allowedContentTypes = [.pdf]
        panel.nameFieldStringValue = "\(activeProjectName).pdf"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        try? data.write(to: url)
    }

    private func adoptProject(id: String, name: String) {
        if let workspaceId = activeWorkspaceId, !showLanding {
            engine.addProjectToWorkspace(workspaceId: workspaceId, projectId: id)
            engine.setWorkspaceActiveProject(workspaceId: workspaceId, projectId: id)
            activeProjectId = id
            showLanding = false
            newProjectOpen = false
            refreshOpenWorkspace(workspaceId)
            applyKnobs()
            engine.fitToScreen()
            maximizeMainWindow()
            return
        }
        if let existing = engine.workspaceForProject(projectId: id),
           let ws = engine.workspace(id: existing)
        {
            openWorkspaceTab(ws)
            switchToProject(workspaceId: ws.id, projectId: id)
            return
        }
        guard let workspaceId = engine.createWorkspaceForProject(projectId: id, name: name),
              let ws = engine.workspace(id: workspaceId)
        else {
            return
        }
        openWorkspaceTab(ws)
        switchToProject(workspaceId: workspaceId, projectId: id)
    }

    private func openWorkspaceTab(_ ws: WorkspaceInfo) {
        if !openWorkspaces.contains(where: { $0.id == ws.id }) {
            openWorkspaces.append(ws)
            persistTabs()
        }
    }

    func openRecent(_ project: ProjectInfo) {
        if let existing = engine.workspaceForProject(projectId: project.id),
           let ws = engine.workspace(id: existing)
        {
            openWorkspaceTab(ws)
            switchToProject(workspaceId: ws.id, projectId: project.id)
            return
        }
        guard let workspaceId = engine.createWorkspaceForProject(
            projectId: project.id,
            name: project.name
        ),
            let ws = engine.workspace(id: workspaceId)
        else {
            return
        }
        openWorkspaceTab(ws)
        switchToProject(workspaceId: workspaceId, projectId: project.id)
    }

    func switchToWorkspace(id: String) {
        if activeWorkspaceId == id, !showLanding {
            return
        }
        guard let ws = openWorkspaces.first(where: { $0.id == id })
            ?? engine.workspace(id: id)
        else {
            return
        }
        openWorkspaceTab(ws)
        engine.touchWorkspace(id: id)
        refreshOpenWorkspace(id)
        let projectId = ws.activeProjectId
            ?? engine.workspaceProjects(workspaceId: id).first?.id
        if let projectId {
            switchToProject(workspaceId: id, projectId: projectId)
        } else {
            engine.switchWorkspace(workspaceId: id, projectId: nil)
            activeWorkspaceId = id
            activeProjectId = nil
            showLanding = false
            newProjectOpen = false
            workspaceExtendOpen = false
            applyKnobs()
            maximizeMainWindow()
        }
    }

    func switchToProject(workspaceId: String, projectId: String) {
        if activeWorkspaceId == workspaceId, activeProjectId == projectId, !showLanding {
            workspaceExtendOpen = false
            return
        }
        engine.switchWorkspace(workspaceId: workspaceId, projectId: projectId)
        if let ws = engine.workspace(id: workspaceId) {
            openWorkspaceTab(ws)
            refreshOpenWorkspace(workspaceId)
        }
        activeWorkspaceId = workspaceId
        activeProjectId = projectId
        showLanding = false
        newProjectOpen = false
        workspaceExtendOpen = false
        applyKnobs()
        engine.fitToScreen()
        engine.syncState()
        engine.refreshLayers()
        maximizeMainWindow()
    }

    func closeWorkspaceTab(_ id: String) {
        openWorkspaces.removeAll { $0.id == id }
        persistTabs()
        if activeWorkspaceId == id {
            engine.save()
            engine.closeProject()
            if let next = openWorkspaces.last {
                switchToWorkspace(id: next.id)
            } else {
                activeWorkspaceId = nil
                activeProjectId = nil
                showLanding = true
            }
        }
    }

    func createEmptyWorkspace(name: String) {
        let resolved = name.isEmpty ? l10n.untitledWorkspace : name
        guard let id = engine.createWorkspace(name: resolved),
              let ws = engine.workspace(id: id)
        else {
            return
        }
        openWorkspaceTab(ws)
        switchToWorkspace(id: id)
    }

    func renameWorkspace(id: String, to name: String) {
        let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        engine.renameWorkspace(id: id, to: trimmed)
        refreshOpenWorkspace(id)
    }

    func setWorkspaceAccent(id: String, color: Color) {
        engine.setWorkspaceAccent(id: id, color: color)
        refreshOpenWorkspace(id)
    }

    func deleteWorkspace(id: String) {
        engine.deleteWorkspace(id: id)
        closeWorkspaceTab(id)
        engine.refreshWorkspaces()
    }

    func deleteAllRecents() {
        engine.deleteAllProjects()
        openWorkspaces = []
        activeWorkspaceId = nil
        activeProjectId = nil
        persistTabs()
        showLanding = true
        newProjectOpen = false
        workspaceExtendOpen = false
    }

    /// Filters act on the active layer, matching `LayerSettingsCard`. Paper is excluded the
    /// same way merge-down and transform already exclude it.
    var canFilterActiveLayer: Bool {
        guard !showLanding, activeProjectId != nil else { return false }
        let index = Int(engine.state.activeLayer)
        guard engine.layerNames.indices.contains(index) else { return false }
        return engine.layerNames[index] != l10n.paper
    }

    func nudgeActiveLayerFilter(_ kind: CalmAdjustment, steps: Float) {
        guard canFilterActiveLayer else { return }
        engine.nudgeLayerAdjustment(Int(engine.state.activeLayer), kind, steps: steps)
    }

    func resetActiveLayerFilters() {
        guard canFilterActiveLayer else { return }
        engine.setLayerAdjustments(Int(engine.state.activeLayer), LayerAdjustments())
    }

    func toggleFullScreen() {
        mainWindow?.toggleFullScreen(nil)
    }

    func selectQuickColor(_ index: Int) {
        guard quickColors.indices.contains(index) else { return }
        activeQuickColorIndex = index
        color = quickColors[index]
        hsb = HSBColor(color)
    }

    /// What the gradient field, hue slider and hex box read and write: always the selected
    /// swatch, since every color the panel offers is one of the three.
    var editedColor: Color {
        get { color }
        set { color = newValue }
    }

    /// A shape's outline is the primary swatch and its interior is the secondary one,
    /// whichever swatch the picker is pointed at — so a rectangle is drawn the way it is
    /// described rather than depending on what was clicked last.
    var shapeStrokeColor: Color { quickColors.first ?? color }
    var shapeFillColor: Color { quickColors.count > 1 ? quickColors[1] : .white }

    func applyEyedropperSample(_ next: Color, at point: CGPoint) {
        let hex = next.hexRGB
        if eyedropperLoupe?.hex != hex {
            color = next
        }
        eyedropperLoupe = EyedropperLoupe(color: next, hex: hex, x: point.x, y: point.y)
    }

    func clearEyedropperLoupe() {
        guard eyedropperLoupe != nil else { return }
        eyedropperLoupe = nil
    }

    func updateHSB(_ next: HSBColor) {
        hsb = next
        editingHSB = true
        editedColor = next.color
        editingHSB = false
    }

    private func maximizeMainWindow() {
        DispatchQueue.main.async { [weak self] in
            guard let window = self?.mainWindow, let screen = window.screen else { return }
            window.setFrame(screen.visibleFrame, display: true, animate: true)
        }
    }

    func applyKnobs() {
        engine.setTool(tool)
        engine.setColor(color)
        engine.setBrush(brushSize)
        engine.setEyedropperRadius(eyedropperRadius)
        engine.setInkOpacity(inkOpacity)
        engine.setBlurStrength(blurStrength)
        engine.setTolerance(tolerance)
        engine.setBrush(brush)
        engine.setEraserHardness(eraserHardness)
        engine.setFill(fill)
        engine.setStroke(stroke)
        engine.setStrokeColor(shapeStrokeColor)
        engine.setShapeFillColor(shapeFillColor)
        engine.setVectorMode(vectorMode)
        engine.setDark(theme.isDark)
        engine.setBoardColors(
            desk: colors.desk,
            grid: colors.deskGrid,
            paperBorder: colors.paperBorder
        )
    }

    func rename(projectId: String, to name: String) {
        let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        engine.rename(projectId: projectId, to: trimmed)
    }

    func setAccent(projectId: String, color: Color) {
        engine.setAccent(projectId: projectId, color: color)
    }

    func selectTool(_ next: CalmTool) {
        // Leaving the text tool ends the session engine-side, which can drop an empty text
        // layer — the panel has to hear about that.
        let wasTyping = engine.textEditing
        tool = next
        if next != .eyedropper {
            clearEyedropperLoupe()
        }
        applyKnobs()
        if wasTyping, next != .text {
            engine.syncState()
            engine.refreshLayers()
        }
    }

    func setTheme(_ next: AppTheme) {
        theme = next
        engine.setDark(theme.isDark)
    }

    func toggleTheme() {
        setTheme(theme == .dark ? .light : .dark)
    }

    func setLanguage(_ next: AppLanguage) {
        language = next
        l10n = L10nCatalog.load(next)
        L10nStore.catalog = l10n
        engine.refreshLayers()
    }
}

struct EyedropperLoupe: Equatable {
    var color: Color
    var hex: String
    var x: CGFloat
    var y: CGFloat
}
