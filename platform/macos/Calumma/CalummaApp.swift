import SwiftUI

@main
struct CalummaApp: App {
    @StateObject private var app = AppModel()

    var body: some Scene {
        WindowGroup(app.l10n.brand) {
            Group {
                if app.showLanding {
                    LandingView()
                } else {
                    EditorView()
                }
            }
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
        .defaultSize(width: 1280, height: 800)
        .windowStyle(.hiddenTitleBar)
        .windowToolbarStyle(.unified)
        .commands {
            CommandGroup(replacing: .newItem) {
                Button(app.l10n.newProjectMenu) { app.showNewProject() }
                    .keyboardShortcut("n", modifiers: [.command])
            }
            CommandMenu(app.l10n.boardMenu) {
                Button(app.l10n.undo) { app.engine.undo() }
                    .keyboardShortcut("z", modifiers: [.command])
                Button(app.l10n.redo) { app.engine.redo() }
                    .keyboardShortcut("z", modifiers: [.command, .shift])
                Divider()
                Button(app.l10n.fitToView) { app.engine.fit() }
                    .keyboardShortcut("0", modifiers: [])
                Button(app.l10n.toggleLayers) { app.layersOpen.toggle() }
                    .keyboardShortcut("l", modifiers: [.command, .option])
                Button(app.l10n.toggleTheme) { app.toggleTheme() }
                    .keyboardShortcut("t", modifiers: [.command])
                Button(app.l10n.settings) { app.settingsOpen = true }
                    .keyboardShortcut(",", modifiers: [.command])
            }
        }
    }
}
