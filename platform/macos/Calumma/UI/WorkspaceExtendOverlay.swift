import SwiftUI

struct WorkspaceExtendOverlay: View {
    @EnvironmentObject private var app: AppModel
    @Environment(\.themeColors) private var colors
    @Environment(\.l10n) private var l10n
    @State private var newName = ""
    @State private var pendingDelete: WorkspaceInfo?

    private var openWorkspaces: [WorkspaceInfo] {
        app.openWorkspaces
    }

    var body: some View {
        VStack(alignment: .leading, spacing: Tokens.Space.md) {
            HStack {
                CalmText.label(l10n.workspaces)
                Spacer()
                HStack(spacing: Tokens.Space.sm) {
                    CalmField(text: $newName)
                        .frame(width: 160)
                    CalmPlainButton(title: l10n.addWorkspace) {
                        app.createEmptyWorkspace(name: newName)
                        newName = ""
                    }
                    Button {
                        app.workspaceExtendOpen = false
                    } label: {
                        Text("×")
                            .font(.system(size: Tokens.TypeSize.title, weight: .medium))
                            .foregroundStyle(colors.textMuted)
                            .frame(width: 24, height: 24)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .calmPointer()
                }
            }

            if openWorkspaces.isEmpty {
                CalmText.muted(l10n.noWorkspaces)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(Tokens.Space.md)
                    .calmSurface()
            } else {
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: Tokens.Space.md) {
                        ForEach(openWorkspaces) { workspace in
                            workspaceCard(workspace)
                        }
                    }
                    .calmScrollBars()
                }
            }
        }
        .padding(Tokens.Space.md)
        .frame(width: 420, height: 480)
        .background(colors.surface)
        .onAppear {
            app.engine.save()
            app.engine.bumpThumbnailRevision()
            if newName.isEmpty { newName = l10n.untitledWorkspace }
        }
        .confirmationDialog(
            l10n.deleteWorkspaceTitle,
            isPresented: Binding(
                get: { pendingDelete != nil },
                set: { if !$0 { pendingDelete = nil } }
            ),
            presenting: pendingDelete
        ) { workspace in
            Button(l10n.delete, role: .destructive) {
                app.deleteWorkspace(id: workspace.id)
            }
            Button(l10n.cancel, role: .cancel) {}
        } message: { workspace in
            Text(l10n.formatKey("deleteWorkspaceNamed", workspace.name))
        }
    }

    private func workspaceCard(_ workspace: WorkspaceInfo) -> some View {
        let projects = app.engine.workspaceProjects(workspaceId: workspace.id)
        let thumbProject = projects.first { $0.id == workspace.activeProjectId } ?? projects.first
        return VStack(alignment: .leading, spacing: Tokens.Space.sm) {
            HStack(spacing: Tokens.Space.sm) {
                Button {
                    app.switchToWorkspace(id: workspace.id)
                } label: {
                    HStack(spacing: Tokens.Space.sm) {
                        if let thumbProject {
                            ProjectThumbView(
                                projectId: thumbProject.id,
                                tint: workspace.accentColor,
                                width: 56,
                                height: 56
                            )
                        } else {
                            CalmThumb(tint: workspace.accentColor, width: 56, height: 56)
                        }
                        VStack(alignment: .leading, spacing: Tokens.Space.xs) {
                            CalmText.title(workspace.name, strong: app.activeWorkspaceId == workspace.id)
                            CalmText.muted(
                                l10n.formatKey("workspaceProjectCount", "\(projects.count)")
                            )
                        }
                        Spacer(minLength: 0)
                    }
                }
                .buttonStyle(.plain)
                .calmPointer()

                Button {
                    pendingDelete = workspace
                } label: {
                    AppIcon.trash(color: colors.danger)
                }
                .buttonStyle(.plain)
                .help(l10n.deleteWorkspace)
                .calmPointer()
            }

            if !projects.isEmpty {
                ScrollView(.horizontal) {
                    HStack(spacing: Tokens.Space.sm) {
                        ForEach(projects) { project in
                            Button {
                                app.switchToProject(workspaceId: workspace.id, projectId: project.id)
                            } label: {
                                VStack(spacing: Tokens.Space.xs) {
                                    ProjectThumbView(
                                        projectId: project.id,
                                        tint: project.accentColor,
                                        width: 72,
                                        height: 72
                                    )
                                    .overlay {
                                        if app.activeProjectId == project.id {
                                            RoundedRectangle(
                                                cornerRadius: Tokens.Radius.sm,
                                                style: .continuous
                                            )
                                            .strokeBorder(colors.accentTeal, lineWidth: 2)
                                        }
                                    }
                                    Text(project.name)
                                        .font(.system(size: Tokens.TypeSize.label, weight: .medium))
                                        .foregroundStyle(colors.textMuted)
                                        .lineLimit(1)
                                        .frame(width: 72)
                                }
                            }
                            .buttonStyle(.plain)
                            .calmPointer()
                        }
                    }
                    .calmScrollBars()
                }
            }
        }
        .padding(Tokens.Space.md)
        .frame(maxWidth: .infinity, alignment: .leading)
        .calmSurface()
    }
}
