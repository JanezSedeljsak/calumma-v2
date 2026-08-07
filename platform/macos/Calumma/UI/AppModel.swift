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
        let info = ProjectInfo(id: id, name: resolved, width: width, height: height, openedAt: 0)
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

    func showNewProject() {
        if activeTabId != nil {
            engine.save()
            engine.closeProject()
        }
        activeTabId = nil
        showLanding = true
        engine.refreshRecents()
    }

    func applyKnobs() {
        engine.setTool(tool)
        engine.setColor(color)
        engine.setBrush(brushSize)
        engine.setFill(fill)
        engine.setDark(theme.isDark)
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
