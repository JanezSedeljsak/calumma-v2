import SwiftUI

struct WorkspaceSettingsCard: View {
    let workspace: WorkspaceInfo

    @EnvironmentObject private var app: AppModel
    @Environment(\.themeColors) private var colors
    @Environment(\.l10n) private var l10n
    @State private var name = ""

    var body: some View {
        VStack(alignment: .leading, spacing: Tokens.Space.md) {
            VStack(alignment: .leading, spacing: Tokens.Space.xs) {
                CalmText.label(l10n.workspaceName)
                CalmField(text: $name)
                    .onSubmit { commitName() }
            }

            VStack(alignment: .leading, spacing: Tokens.Space.sm) {
                CalmText.label(l10n.workspaceColor)
                CalmPaletteRow(colors: Engine.palette, selected: current) { color in
                    app.setWorkspaceAccent(id: workspace.id, color: color)
                }
            }

            HStack {
                Spacer()
                CalmPlainButton(title: l10n.done, accent: true) { commitName() }
            }
        }
        .padding(Tokens.Space.md)
        .frame(width: 280)
        .background(colors.surface)
        .onAppear { name = workspace.name }
    }

    private var current: Color {
        app.openWorkspaces.first { $0.id == workspace.id }?.accentColor ?? workspace.accentColor
    }

    private func commitName() {
        app.renameWorkspace(id: workspace.id, to: name)
    }
}
