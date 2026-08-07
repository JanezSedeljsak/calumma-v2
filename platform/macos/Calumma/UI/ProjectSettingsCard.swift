import SwiftUI

struct ProjectSettingsCard: View {
    let project: ProjectInfo

    @EnvironmentObject private var app: AppModel
    @Environment(\.themeColors) private var colors
    @Environment(\.l10n) private var l10n
    @State private var name = ""

    var body: some View {
        VStack(alignment: .leading, spacing: Tokens.Space.md) {
            VStack(alignment: .leading, spacing: Tokens.Space.xs) {
                CalmText.label(l10n.projectName)
                CalmField(text: $name)
                    .onSubmit { commitName() }
            }

            VStack(alignment: .leading, spacing: Tokens.Space.sm) {
                CalmText.label(l10n.projectColor)
                CalmPaletteRow(colors: Engine.palette, selected: current) { color in
                    app.setAccent(projectId: project.id, color: color)
                }
            }

            HStack {
                CalmText.muted("\(project.width) × \(project.height)")
                Spacer()
                CalmPlainButton(title: l10n.done, accent: true) { commitName() }
            }
        }
        .padding(Tokens.Space.md)
        .frame(width: 280)
        .background(colors.surface)
        .onAppear { name = project.name }
    }

    private var current: Color {
        app.openTabs.first { $0.id == project.id }?.accentColor ?? project.accentColor
    }

    private func commitName() {
        app.rename(projectId: project.id, to: name)
    }
}
