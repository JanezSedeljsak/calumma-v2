import SwiftUI

enum CalmWindowId {
    static let newProject = "new-project"
}

@main
struct CalummaApp: App {
    @StateObject private var app = AppModel()

    var body: some Scene {
        WindowGroup(app.l10n.brand) {
            Group {
                if app.showLanding {
                    NewProjectView()
                } else {
                    EditorView()
                }
            }
            .frame(minWidth: Tokens.Window.mainMinWidth, minHeight: Tokens.Window.mainMinHeight)
            .background(TitleBarZoomOnDoubleClick())
            .environmentObject(app)
            .themeColors(app.colors)
            .l10n(app.l10n)
            .preferredColorScheme(app.theme.isDark ? .dark : .light)
            .sheet(isPresented: $app.settingsOpen) {
                SettingsView()
                    .environmentObject(app)
                    .themeColors(app.colors)
                    .l10n(app.l10n)
            }
        }
        .defaultSize(width: Tokens.Window.mainWidth, height: Tokens.Window.mainHeight)
        .windowStyle(.hiddenTitleBar)
        .windowToolbarStyle(.unifiedCompact)
        .commands {
            CommandGroup(replacing: .newItem) {
                NewProjectCommand(title: app.l10n.newProjectMenu, disabled: app.showLanding)
            }
            CommandGroup(replacing: .undoRedo) {
                Button(app.l10n.undo) { app.engine.undo() }
                    .keyboardShortcut("z", modifiers: [.command])
                    .disabled(!app.engine.state.canUndo)
                Button(app.l10n.redo) { app.engine.redo() }
                    .keyboardShortcut("z", modifiers: [.command, .shift])
                    .disabled(!app.engine.state.canRedo)
            }
            CommandMenu(app.l10n.boardMenu) {
                Button(app.l10n.fitToView) { app.engine.fit() }
                    .keyboardShortcut("0", modifiers: [])
                Button(app.l10n.toggleLayers) { app.layersOpen.toggle() }
                    .keyboardShortcut("l", modifiers: [.command, .option])
                Button(app.l10n.settings) { app.settingsOpen = true }
                    .keyboardShortcut(",", modifiers: [.command])
            }
        }

        Window(app.l10n.newProject, id: CalmWindowId.newProject) {
            NewProjectWindow()
                .environmentObject(app)
                .themeColors(app.colors)
                .l10n(app.l10n)
                .preferredColorScheme(app.theme.isDark ? .dark : .light)
        }
        .defaultSize(width: Tokens.Window.newProjectWidth, height: Tokens.Window.newProjectHeight)
        .windowResizability(.contentMinSize)
    }
}

private struct NewProjectCommand: View {
    @Environment(\.openWindow) private var openWindow
    let title: String
    let disabled: Bool

    var body: some View {
        Button(title) { openWindow(id: CalmWindowId.newProject) }
            .keyboardShortcut("n", modifiers: [.command])
            .disabled(disabled)
    }
}

private struct NewProjectWindow: View {
    @EnvironmentObject private var app: AppModel
    @Environment(\.dismissWindow) private var dismissWindow

    var body: some View {
        NewProjectView(isLanding: false)
            .frame(
                minWidth: Tokens.Window.newProjectMinWidth,
                minHeight: Tokens.Window.newProjectMinHeight
            )
            .onChange(of: app.activeTabId) { _, _ in
                dismissWindow(id: CalmWindowId.newProject)
            }
    }
}
