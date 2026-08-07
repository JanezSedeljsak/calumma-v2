import SwiftUI

@main
struct CalummaApp: App {
    @StateObject private var app = AppModel()

    var body: some Scene {
        WindowGroup(app.l10n.brand) {
            Group {
                if app.showLanding {
                    NewProjectView()
                        .frame(width: Tokens.Window.mainWidth, height: Tokens.Window.mainHeight)
                } else {
                    EditorView()
                        .frame(minWidth: Tokens.Window.mainMinWidth, minHeight: Tokens.Window.mainMinHeight)
                }
            }
            .background(TitleBarZoomOnDoubleClick())
            .background(WindowAccessor { window in app.mainWindow = window })
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
        .windowResizability(.contentSize)
        .windowStyle(.hiddenTitleBar)
        .windowToolbarStyle(.unifiedCompact)
        .commands {
            CommandGroup(replacing: .newItem) {
                Button(app.l10n.newProjectMenu) { app.newProjectOpen = true }
                    .keyboardShortcut("n", modifiers: [.command])
                    .disabled(app.showLanding)
            }
            CommandGroup(after: .newItem) {
                Menu(app.l10n.exportMenu) {
                    ForEach(ExportFormat.allCases) { format in
                        Button(app.l10n.formatKey("exportAs", format.label)) {
                            app.exportComposite(as: format)
                        }
                    }
                    Button(app.l10n.formatKey("exportAs", "PSD")) { app.exportPSD() }
                }
                .disabled(app.showLanding)
            }
            CommandGroup(replacing: .undoRedo) {
                Button(app.l10n.undo) { app.engine.undo() }
                    .keyboardShortcut("z", modifiers: [.command])
                    .disabled(!app.engine.state.canUndo)
                Button(app.l10n.redo) { app.engine.redo() }
                    .keyboardShortcut("z", modifiers: [.command, .shift])
                    .disabled(!app.engine.state.canRedo)
            }
            CommandGroup(replacing: .appSettings) {
                Button(app.l10n.settings) { app.settingsOpen = true }
                    .keyboardShortcut(",", modifiers: [.command])
            }
            CommandMenu(app.l10n.boardMenu) {
                Button(app.l10n.fitToView) { app.engine.fit() }
                    .keyboardShortcut("0", modifiers: [])
                Button(app.l10n.toggleLayers) { app.layersOpen.toggle() }
                    .keyboardShortcut("l", modifiers: [.command, .option])
            }
        }
    }
}
