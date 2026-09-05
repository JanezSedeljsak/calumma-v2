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
    @Published var openProjects: [ProjectInfo] = []
    @Published var activeProjectId: String?
    /// The project the canvas is waiting on, and the only reason the board shows a skeleton.
    /// Carries the whole `ProjectInfo` rather than an id because the skeleton is drawn at that
    /// project's own size — see `CanvasSkeleton`.
    @Published private(set) var loadingProject: ProjectInfo?
    @Published var showLanding = true
    @Published var newProjectOpen = false
    @Published var settingsOpen = false
    @Published var guidesOpen = false

    /// Whether anything is covering the board. The board dresses the cursor and rings the brush
    /// off its own tracking area, which keeps firing underneath a SwiftUI overlay — so it has to
    /// be told to stand down, or a modal is a panel with no pointer on it.
    var modalPresented: Bool { newProjectOpen || settingsOpen || guidesOpen }
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
    @Published var cloneAligned: Bool = Engine.cloneAlignedDefault
    @Published var tolerance: UInt8 = Engine.toleranceDefault
    @Published var brush: CalmBrush = .pen
    @Published var eraserHardness: Float = Engine.eraserHardnessDefault
    @Published var fill = false
    @Published var stroke = true
    @Published var vectorMode = false
    /// `nil` is a free-form crop drag; a ratio locks `Tool::Crop`'s rect to it.
    @Published var cropAspectLock: Float? {
        didSet { engine.setCropAspectLock(cropAspectLock) }
    }
    @Published var cropOverlayStyle: CalmCropOverlayStyle = .off {
        didSet { engine.setCropOverlayStyle(cropOverlayStyle) }
    }
    @Published var straightening = false {
        didSet { engine.setStraightenActive(straightening) }
    }
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
        restoreOpenProjectTabs()
    }

    private func restoreOpenProjectTabs() {
        let ids = engine.loadOpenProjectTabs()
        guard !ids.isEmpty else { return }
        openProjects = ids.compactMap { engine.project(id: $0) }
        if let first = openProjects.first {
            switchToProject(id: first.id)
        }
    }

    private func persistTabs() {
        engine.persistOpenProjectTabs(openProjects.map(\.id))
    }

    private func refreshOpenProject(_ id: String) {
        guard let updated = engine.project(id: id) else { return }
        if let index = openProjects.firstIndex(where: { $0.id == id }) {
            openProjects[index] = updated
        }
    }

    private func openProjectTab(_ project: ProjectInfo) {
        if !openProjects.contains(where: { $0.id == project.id }) {
            openProjects.append(project)
            persistTabs()
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
        let artworks = ArtworkImport.fromPasteboardAll()
        guard !artworks.isEmpty else { return }
        if showLanding || activeProjectId == nil {
            createFromArtworks(artworks)
        } else {
            pasteIntoBoard(artworks)
        }
    }

    /// A pasted image lands as a new layer at the top of the stack, and the first thing anyone
    /// does with one is put it somewhere — so the board hands it over already grabbed: Move
    /// selected, the new layer inside `⌘T` with its corners live. Dropping onto an open board
    /// is the same gesture by another route, so it goes through here too.
    private func pasteIntoBoard(_ artworks: [ArtworkImage]) {
        let (count, outcome) = engine.pasteImages(artworks)
        guard count > 0, outcome != .failed else {
            showToast(l10n.pasteFailed, kind: .error)
            return
        }
        enterMoveTransform()
        if outcome == .overflowing {
            showToast(l10n.pasteOverflows, kind: .success)
        }
    }

    /// Idempotent, unlike picking Move: paste and drop want the new layer grabbed with handles
    /// live whether or not Move was already the tool.
    func enterMoveTransform() {
        selectTool(.move)
        engine.enterTransform()
    }

    func toggleMoveTransform() {
        guard tool == .move else {
            enterMoveTransform()
            return
        }
        engine.toggleTransform()
    }

    func setMoveTransform(_ on: Bool) {
        if on {
            engine.enterTransform()
        } else {
            engine.exitTransform()
        }
    }

    func createFromArtwork(_ artwork: ArtworkImage?) {
        guard let artwork else { return }
        createFromArtworks([artwork])
    }

    func createFromArtworks(_ artworks: [ArtworkImage]) {
        guard !artworks.isEmpty else { return }
        let name = artworks[0].name
        guard let id = engine.createProjectFromImages(name: name, artworks: artworks) else {
            artworkError = l10n.artworkImportFailed
            return
        }
        artworkError = nil
        adoptProject(id: id, name: name)
    }

    func importArtwork(url: URL) {
        createFromArtwork(ArtworkImport.decode(url: url))
    }

    func pasteArtwork() {
        createFromArtworks(ArtworkImport.fromPasteboardAll())
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

    @discardableResult
    func dropArtwork(providers: [NSItemProvider]) -> Bool {
        ArtworkImport.loadAll(providers: providers) { [weak self] artworks in
            guard !artworks.isEmpty else { return }
            self?.createFromArtworks(artworks)
        }
    }

    @discardableResult
    func dropArtworkIntoBoard(providers: [NSItemProvider]) -> Bool {
        ArtworkImport.loadAll(providers: providers) { [weak self] artworks in
            guard !artworks.isEmpty else { return }
            self?.pasteIntoBoard(artworks)
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
    private func showToast(
        _ text: String,
        kind: ToastKind,
        actionTitle: String? = nil,
        action: (() -> Void)? = nil
    ) {
        let message = ToastMessage(text: text, kind: kind, actionTitle: actionTitle, action: action)
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
        return l10n.untitled
    }

    func exportComposite(as format: ExportFormat) {
        guard let data = engine.exportImage(format: format) else { return }
        let panel = NSSavePanel()
        panel.allowedContentTypes = [format.utType]
        panel.nameFieldStringValue = "\(activeProjectName).\(format.fileExtension)"
        guard panel.runModal() == .OK, let url = panel.url else { return }
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
              let data = engine.exportLayerImage(index: index, format: format)
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
        guard let project = engine.project(id: id) else { return }
        openProjectTab(project)
        switchToProject(id: id)
    }

    func openRecent(_ project: ProjectInfo) {
        openProjectTab(project)
        switchToProject(id: project.id)
    }

    func switchToProject(id: String) {
        if activeProjectId == id, !showLanding {
            return
        }
        guard let project = openProjects.first(where: { $0.id == id })
            ?? engine.project(id: id)
        else {
            return
        }
        openProjectTab(project)
        activeProjectId = id
        showLanding = false
        newProjectOpen = false
        beginLoading(project)
    }

    /// How long the canvas holds its skeleton at minimum. A project reads back out of SQLite
    /// faster than that on anything small, and a placeholder that appears and vanishes inside
    /// one frame reads as a flicker rather than as a load — so the skeleton is given long
    /// enough to be seen, and the board underneath it is already finished by then.
    private static let skeletonMinSeconds = 0.2

    /// The tab lights up now; the board catches up a moment later. Opening a project reads its
    /// layers back out of SQLite, and doing that inline is what made clicking a tab freeze the
    /// window mid-click. The load is handed to the next runloop turn so the skeleton gets
    /// painted first, and the skeleton is drawn at the incoming project's aspect ratio — so the
    /// switch lands on a board the right shape instead of on the previous project's picture.
    private func beginLoading(_ project: ProjectInfo) {
        loadingProject = project
        let ready = DispatchTime.now() + Self.skeletonMinSeconds
        DispatchQueue.main.async { [weak self] in
            guard let self, loadingProject?.id == project.id else { return }
            loadProject(id: project.id)
            DispatchQueue.main.asyncAfter(deadline: ready) { [weak self] in
                guard let self, loadingProject?.id == project.id else { return }
                withAnimation(.easeOut(duration: 0.18)) { self.loadingProject = nil }
            }
        }
    }

    private func loadProject(id: String) {
        engine.openProject(id: id)
        refreshOpenProject(id)
        applyKnobs()
        engine.fitToScreen()
        engine.syncState()
        engine.refreshLayers()
        maximizeMainWindow()
    }

    func closeProjectTab(_ id: String) {
        openProjects.removeAll { $0.id == id }
        persistTabs()
        if activeProjectId == id {
            engine.save()
            engine.closeProject()
            if let next = openProjects.last {
                switchToProject(id: next.id)
            } else {
                activeProjectId = nil
                showLandingScreen()
            }
        }
    }

    func deleteProject(id: String) {
        openProjects.removeAll { $0.id == id }
        persistTabs()
        if activeProjectId == id {
            engine.save()
            engine.closeProject()
            activeProjectId = nil
            showLandingScreen()
        }
        engine.deleteProject(id: id)
    }

    func deleteAllRecents() {
        engine.deleteAllProjects()
        openProjects = []
        activeProjectId = nil
        persistTabs()
        showLandingScreen()
        newProjectOpen = false
    }

    /// Back to Landing, and the one place a pending load is abandoned: whatever the canvas was
    /// waiting on, it is not going to be shown now, and leaving `loadingProject` set would let
    /// the deferred open reinstate a project that was just closed or deleted.
    private func showLandingScreen() {
        loadingProject = nil
        showLanding = true
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
        engine.setCloneAligned(cloneAligned)
        engine.setTolerance(tolerance)
        engine.setBrush(brush)
        engine.setEraserHardness(eraserHardness)
        engine.setFill(fill)
        engine.setStroke(stroke)
        engine.setStrokeColor(shapeStrokeColor)
        engine.setShapeFillColor(shapeFillColor)
        if quickColors.count > 2 {
            engine.setSelectColor(quickColors[2])
        }
        engine.setVectorMode(vectorMode)
        engine.setDark(theme.isDark)
        engine.setBoardColors(
            desk: colors.desk,
            grid: colors.deskGrid,
            paperBorder: colors.paperBorder
        )
    }

    func syncMatchColorFromEngine() {
        guard quickColors.count > 2 else { return }
        let next = engine.matchColor
        if quickColors[2] != next {
            quickColors[2] = next
        }
    }

    func rename(projectId: String, to name: String) {
        let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        engine.rename(projectId: projectId, to: trimmed)
        refreshOpenProject(projectId)
    }

    func setAccent(projectId: String, color: Color) {
        engine.setAccent(projectId: projectId, color: color)
        refreshOpenProject(projectId)
    }

    /// Applies the crop rect and stays on `Tool::Crop` with a fresh full-canvas rect, the same
    /// way committing a shape leaves its tool selected.
    func commitCrop() {
        engine.commitCrop()
    }

    /// Leaves Crop for the tool most people reach for next, without applying anything.
    func cancelCrop() {
        straightening = false
        selectTool(.move)
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
