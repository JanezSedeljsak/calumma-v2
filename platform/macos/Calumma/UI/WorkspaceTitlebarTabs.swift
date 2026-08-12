import SwiftUI

struct WorkspaceTitlebarTabs: View {
    @EnvironmentObject private var app: AppModel
    @Environment(\.themeColors) private var colors
    @Environment(\.l10n) private var l10n
    @Binding var editingTab: String?

    var body: some View {
        HStack(spacing: 0) {
            ForEach(app.openWorkspaces) { tab in
                tabChip(tab)
            }

            titlebarIconButton(help: l10n.newProject) {
                app.newProjectOpen = true
            } icon: {
                AppIcon.plus(color: colors.textMuted, size: 14)
            }

            titlebarIconButton(help: l10n.showWorkspaces) {
                app.workspaceExtendOpen.toggle()
            } icon: {
                AppIcon.more(color: colors.textMuted)
            }
            .popover(
                isPresented: $app.workspaceExtendOpen,
                arrowEdge: .bottom
            ) {
                WorkspaceExtendOverlay()
                    .environmentObject(app)
                    .themeColors(colors)
                    .l10n(l10n)
            }
        }
        .padding(.horizontal, Tokens.Space.xs)
        .frame(height: 28)
        .background(colors.surface, in: Capsule(style: .continuous))
        .clipShape(Capsule(style: .continuous))
    }

    private func tabChip(_ tab: WorkspaceInfo) -> some View {
        let selected = app.activeWorkspaceId == tab.id
        return HStack(spacing: Tokens.Space.xs) {
            Button {
                editingTab = tab.id
            } label: {
                CalmDot(color: tab.accentColor)
            }
            .buttonStyle(.plain)

            Button {
                app.switchToWorkspace(id: tab.id)
            } label: {
                Text(tab.name)
                    .font(.system(size: Tokens.TypeSize.label, weight: selected ? .semibold : .medium))
                    .foregroundStyle(selected ? colors.text : colors.textMuted)
                    .lineLimit(1)
                    .truncationMode(.tail)
            }
            .buttonStyle(.plain)
            .help(tab.name)

            Button {
                app.closeWorkspaceTab(tab.id)
            } label: {
                Text("×")
                    .font(.system(size: Tokens.TypeSize.label, weight: .medium))
                    .foregroundStyle(colors.textMuted)
            }
            .buttonStyle(.plain)
        }
        .padding(.horizontal, Tokens.Space.sm)
        .frame(maxHeight: .infinity)
        .contentShape(Rectangle())
        .background(selected ? colors.surfaceHover : Color.clear)
        .calmPointer()
        .popover(
            isPresented: Binding(
                get: { editingTab == tab.id },
                set: { if !$0 { editingTab = nil } }
            ),
            arrowEdge: .bottom
        ) {
            WorkspaceSettingsCard(workspace: tab)
                .environmentObject(app)
                .themeColors(colors)
                .l10n(l10n)
        }
    }

    private func titlebarIconButton<Icon: View>(
        help: String,
        action: @escaping () -> Void,
        @ViewBuilder icon: () -> Icon
    ) -> some View {
        Button(action: action) {
            icon()
                .frame(width: 28, height: 28)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(help)
        .calmPointer()
    }
}
