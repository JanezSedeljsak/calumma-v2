import Combine
import SwiftUI

@MainActor
final class AppModel: ObservableObject {
    let engine: Engine
    @Published var theme: AppTheme = .dark
    @Published var language: AppLanguage = .en
    @Published private(set) var l10n: L10nCatalog = .load(.en)
    @Published var openTabs: [ProjectInfo] = []
    @Published var activeTabId: String?
    @Published var showLanding = true
    @Published var settingsOpen = false
    @Published var artworkError: String?

    @Published var tool: CalmTool = .pen
    @Published var lastShapeTool: CalmTool = .rect
    @Published var color: Color = Color(red: 0.1, green: 0.1, blue: 0.1)
    @Published var brushSize: Float = 3
    @Published var fill = false
    @Published var layersOpen = true
    @Published var spacePan = false

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
    }

    func create(name: String, width: Int, height: Int) {
        let resolved = name.isEmpty ? l10n.untitled : name
        guard let id = engine.createProject(name: resolved, width: width, height: height) else {
            return
        }
        adopt(id: id, name: resolved, width: width, height: height)
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

    func clearArtworkError() {
        artworkError = nil
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
        applyKnobs()
        engine.fit()
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
        applyKnobs()
        engine.fit()
        engine.syncState()
        engine.refreshLayers()
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
