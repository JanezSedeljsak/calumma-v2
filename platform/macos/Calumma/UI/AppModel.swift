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
    @Published var openTabs: [ProjectInfo] = []
    @Published var activeTabId: String?
    @Published var showLanding = true
    @Published var newProjectOpen = false
    @Published var settingsOpen = false
    @Published var artworkError: String?

    @Published var tool: CalmTool = .pen
    @Published var lastShapeTool: CalmTool = .rect
    @Published var lastSelectTool: CalmTool = .selectRect
    @Published var color: Color = Color(red: 0.1, green: 0.1, blue: 0.1) {
        didSet {
            quickColors[activeQuickColorIndex] = color
            if !editingHSB {
                hsb = HSBColor(color)
            }
        }
    }
    @Published var quickColors: [Color] = [
        Color(red: 0.1, green: 0.1, blue: 0.1),
        Color.white,
    ]
    @Published var activeQuickColorIndex = 0
    @Published private(set) var hsb = HSBColor(Color(red: 0.1, green: 0.1, blue: 0.1))
    private var editingHSB = false
    @Published var brushSize: Float = 3
    @Published var fill = false
    @Published var layersOpen = true
    @Published var spacePan = false

    weak var mainWindow: NSWindow?

    private var cancellables = Set<AnyCancellable>()

    var colors: ThemeColors { ThemeColors.colors(for: theme) }

    init() {
        engine = Engine()
        L10nStore.catalog = l10n
        engine.objectWillChange
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in
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
    }

    func create(name: String, width: Int, height: Int, accent: Color? = nil) {
        let resolved = name.isEmpty ? l10n.untitled : name
        guard let id = engine.createProject(name: resolved, width: width, height: height) else {
            return
        }
        if let accent {
            engine.setAccent(projectId: id, color: accent)
        }
        adopt(id: id, name: resolved, width: width, height: height)
    }

    func copySelectionOrCanvas() {
        let image = engine.hasSelection ? engine.selectionCGImage() : engine.compositeCGImage()
        guard let image, let data = ImageEncode.pngData(image) else { return }
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.setData(data, forType: .png)
    }

    func copyLayer(index: Int) {
        if engine.isLayerVector(index: index), let svg = engine.layerSVG(index: index) {
            let pasteboard = NSPasteboard.general
            pasteboard.clearContents()
            pasteboard.setString(svg, forType: NSPasteboard.PasteboardType("public.svg-image"))
            return
        }
        guard let image = engine.layerCGImage(index: index), let data = ImageEncode.pngData(image)
        else {
            return
        }
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.setData(data, forType: .png)
    }

    func cutSelection() {
        guard engine.hasSelection,
              let image = engine.selectionCGImage(),
              let data = ImageEncode.pngData(image)
        else {
            return
        }
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.setData(data, forType: .png)
        engine.clearSelectionPixels()
    }

    func pasteFromClipboard() {
        guard let artwork = ArtworkImport.fromPasteboard() else { return }
        engine.pasteImage(
            premultipliedRGBA: artwork.premultipliedRGBA,
            width: artwork.width,
            height: artwork.height
        )
    }

    func createFromArtwork(_ artwork: ArtworkImage?) {
        guard let artwork, let id = engine.createProject(name: artwork.name, artwork: artwork)
        else {
            artworkError = l10n.artworkImportFailed
            return
        }
        artworkError = nil
        adopt(id: id, name: artwork.name, width: artwork.width, height: artwork.height)
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
            self?.engine.pasteImage(
                premultipliedRGBA: artwork.premultipliedRGBA,
                width: artwork.width,
                height: artwork.height
            )
        }
    }

    func clearArtworkError() {
        artworkError = nil
    }

    private var activeProjectName: String {
        openTabs.first { $0.id == activeTabId }?.name ?? l10n.untitled
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

    func exportPSD() {
        guard let data = engine.exportPSD() else { return }
        let panel = NSSavePanel()
        panel.allowedContentTypes = [UTType(filenameExtension: "psd") ?? .data]
        panel.nameFieldStringValue = "\(activeProjectName).psd"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        try? data.write(to: url)
    }

    private func adopt(id: String, name: String, width: Int, height: Int) {
        let info = ProjectInfo(
            id: id,
            name: name,
            width: width,
            height: height,
            openedAt: 0,
            accent: engine.state.accent
        )
        openTabs.removeAll { $0.id == id }
        openTabs.append(info)
        activeTabId = id
        showLanding = false
        newProjectOpen = false
        applyKnobs()
        engine.fit()
        maximizeMainWindow()
    }

    func selectQuickColor(_ index: Int) {
        guard quickColors.indices.contains(index) else { return }
        activeQuickColorIndex = index
        color = quickColors[index]
    }

    func updateHSB(_ next: HSBColor) {
        hsb = next
        editingHSB = true
        color = next.color
        editingHSB = false
    }

    private func maximizeMainWindow() {
        DispatchQueue.main.async { [weak self] in
            guard let window = self?.mainWindow, let screen = window.screen else { return }
            window.setFrame(screen.visibleFrame, display: true, animate: true)
        }
    }

    func openRecent(_ project: ProjectInfo) {
        switchTo(projectId: project.id, info: project)
    }

    func switchTo(projectId: String, info: ProjectInfo? = nil) {
        if activeTabId == projectId, !showLanding {
            return
        }
        if let activeTabId {
            engine.save()
            _ = activeTabId
        }
        engine.closeProject()
        engine.openProject(id: projectId)
        if let info {
            if !openTabs.contains(where: { $0.id == info.id }) {
                openTabs.append(info)
            }
        } else if let recent = engine.recents.first(where: { $0.id == projectId }),
                  !openTabs.contains(where: { $0.id == projectId })
        {
            openTabs.append(recent)
        }
        activeTabId = projectId
        showLanding = false
        newProjectOpen = false
        applyKnobs()
        engine.fit()
        engine.syncState()
        engine.refreshLayers()
        maximizeMainWindow()
    }

    func closeTab(_ id: String) {
        openTabs.removeAll { $0.id == id }
        if activeTabId == id {
            engine.save()
            engine.closeProject()
            if let next = openTabs.last {
                switchTo(projectId: next.id, info: next)
            } else {
                activeTabId = nil
                showLanding = true
            }
        }
    }

    func applyKnobs() {
        engine.setTool(tool)
        engine.setColor(color)
        engine.setBrush(brushSize)
        engine.setFill(fill)
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
        replaceTab(projectId) { ProjectInfo(
            id: $0.id,
            name: trimmed,
            width: $0.width,
            height: $0.height,
            openedAt: $0.openedAt,
            accent: $0.accent
        ) }
    }

    func setAccent(projectId: String, color: Color) {
        engine.setAccent(projectId: projectId, color: color)
        replaceTab(projectId) { ProjectInfo(
            id: $0.id,
            name: $0.name,
            width: $0.width,
            height: $0.height,
            openedAt: $0.openedAt,
            accent: color.packedRGB
        ) }
    }

    private func replaceTab(_ id: String, _ transform: (ProjectInfo) -> ProjectInfo) {
        guard let index = openTabs.firstIndex(where: { $0.id == id }) else { return }
        openTabs[index] = transform(openTabs[index])
    }

    func selectTool(_ next: CalmTool) {
        tool = next
        if next.isShape {
            lastShapeTool = next
        }
        if next.isSelection {
            lastSelectTool = next
        }
        applyKnobs()
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
