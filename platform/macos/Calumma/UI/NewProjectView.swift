import AppKit
import SwiftUI

struct NewProjectView: View {
    var isLanding = true

    @EnvironmentObject private var app: AppModel
    @Environment(\.themeColors) private var colors
    @Environment(\.l10n) private var l10n
    @State private var name = ""
    @State private var width = 1280
    @State private var height = 720
    @State private var accent: Color = Engine.palette.randomElement() ?? .gray
    @State private var colorPickerOpen = false
    @State private var pendingDelete: ProjectInfo?
    @State private var pendingClearAll = false
    @State private var hoveredRecent: String?

    var body: some View {
        GeometryReader { geo in
            landscapeLayout(size: geo.size)
                .padding(Tokens.Space.lg)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        }
        .calmScreen()
        .background(ArtworkPasteCatcher(app: app))
        .onAppear {
            if name.isEmpty { name = l10n.newProject }
            app.clearArtworkError()
            app.engine.refreshRecents()
        }
    }

    private func landscapeLayout(size: CGSize) -> some View {
        HStack(alignment: .top, spacing: Tokens.Space.lg) {
            VStack(alignment: .leading, spacing: Tokens.Space.lg) {
                header
                ScrollView(.vertical) {
                    VStack(alignment: .leading, spacing: Tokens.Space.lg) {
                        createForm
                        HStack(alignment: .top, spacing: Tokens.Space.lg) {
                            presetsColumn
                            recentsColumn
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .topLeading)
                    .calmScrollBars()
                }
            }
            .frame(maxWidth: .infinity, alignment: .topLeading)

            PasteArtworkIsland()
                .frame(width: pasteWidth(for: size))
                .frame(maxHeight: .infinity)
        }
    }

    private func pasteWidth(for size: CGSize) -> CGFloat {
        min(
            max(size.width * Tokens.Window.pasteWidthRatio, Tokens.Window.pasteMinWidth),
            Tokens.Window.pasteMaxWidth
        )
    }

    private var header: some View {
        HStack(alignment: .firstTextBaseline, spacing: Tokens.Space.sm) {
            CalmText.brand(l10n.brand)
            CalmText.eyebrow(l10n.tagline)
            Spacer()
            Image(nsImage: NSImage(named: "AppIcon") ?? NSImage())
                .resizable()
                .frame(width: Tokens.TypeSize.brand, height: Tokens.TypeSize.brand)
                .alignmentGuide(.firstTextBaseline) { $0.height * 0.9 }
        }
        .lineLimit(1)
        .minimumScaleFactor(0.8)
    }

    private var createForm: some View {
        VStack(alignment: .leading, spacing: Tokens.Space.lg) {
            VStack(alignment: .leading, spacing: Tokens.Space.xs) {
                CalmText.label(l10n.projectName)
                HStack(spacing: Tokens.Space.sm) {
                    Button {
                        colorPickerOpen = true
                    } label: {
                        CalmDot(color: accent, size: 16)
                    }
                    .buttonStyle(.plain)
                    .calmPointer()
                    .help(l10n.projectColor)
                    .popover(isPresented: $colorPickerOpen, arrowEdge: .bottom) {
                        CalmPaletteRow(colors: Engine.palette, selected: accent) { color in
                            accent = color
                            colorPickerOpen = false
                        }
                        .padding(Tokens.Space.sm)
                    }

                    CalmField(text: $name)
                        .onSubmit(createProject)
                }
            }

            VStack(alignment: .leading, spacing: Tokens.Space.xs) {
                CalmText.label(l10n.resolution)
                HStack(spacing: Tokens.Space.sm) {
                    CalmNumberField(value: $width, width: 72)
                    CalmNumberField(value: $height, width: 72)
                    CalmAccentButton(title: l10n.create, action: createProject)
                }
            }
        }
    }

    /// Return in the name field creates the project, which is the same thing the Create
    /// button does — typing a name and pressing Return is the whole form for most projects,
    /// and an empty name still resolves to Untitled in `AppModel.create`.
    private func createProject() {
        app.create(name: name, width: width, height: height, accent: accent)
    }

    private var presetsColumn: some View {
        CalmSection(title: l10n.presets, accent: colors.accentTeal, contentSpacing: Tokens.Space.sm) {
            ForEach(Tokens.presets) { preset in
                CalmRowButton {
                    app.create(name: preset.label, width: preset.width, height: preset.height, accent: accent)
                } content: {
                    CalmRow(
                        leading: { AppIcon.plus(color: colors.textMuted) },
                        title: preset.label,
                        subtitle: "\(preset.width) × \(preset.height)",
                        useTitleSize: true
                    )
                }
            }
        }
    }

    /// A project that already has a tab is not something you can open — it is open. Listing it
    /// here would offer a second, slower way to select the tab you are already looking at, so
    /// Recents is what is *not* on screen yet.
    private var recents: [ProjectInfo] {
        let open = Set(app.openProjects.map(\.id))
        return app.engine.recents.filter { !open.contains($0.id) }
    }

    private var recentsColumn: some View {
        CalmSection(title: l10n.recents, accent: colors.accentOrange, contentSpacing: Tokens.Space.sm) {
            if !recents.isEmpty {
                Button(l10n.clearAllRecents) {
                    pendingClearAll = true
                }
                .buttonStyle(.plain)
                .font(.system(size: Tokens.TypeSize.label))
                .foregroundStyle(colors.danger)
                .calmPointer()
            }
        } content: {
            if recents.isEmpty {
                CalmText.muted(l10n.noRecents)
                    .padding(Tokens.Space.sm)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .calmSurface(bordered: true)
            } else {
                ForEach(recents.prefix(5)) { project in
                    let hovered = hoveredRecent == project.id
                    HStack(spacing: Tokens.Space.sm) {
                        Button {
                            app.openRecent(project)
                        } label: {
                            CalmRow(
                                leading: {
                                    ProjectThumbView(
                                        projectId: project.id,
                                        tint: project.accentColor,
                                        width: 36,
                                        height: 36
                                    )
                                },
                                title: project.name,
                                subtitle: "\(project.width) × \(project.height)",
                                trailing: relativeTime(project.openedAt)
                            )
                        }
                        .buttonStyle(.plain)
                        .calmPointer()

                        // Revealed by hover like the layer rows, and holding its slot either
                        // way so the card does not reflow under the pointer. Unlike a layer,
                        // a deleted project is gone from SQLite for good — so this one keeps
                        // its confirmation.
                        Button {
                            pendingDelete = project
                        } label: {
                            AppIcon.trash(color: colors.danger)
                        }
                        .buttonStyle(.plain)
                        .help(l10n.deleteProject)
                        .calmPointer(hovered)
                        .opacity(hovered ? 1 : 0)
                        .allowsHitTesting(hovered)
                        .animation(.easeOut(duration: 0.12), value: hovered)
                    }
                    .padding(.horizontal, Tokens.Space.md)
                    .padding(.vertical, Tokens.Space.sm)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .calmSurface(bordered: true)
                    .onHover { hoveredRecent = $0 ? project.id : nil }
                }
            }
        }
        .confirmationDialog(
            l10n.deleteProjectTitle,
            isPresented: Binding(
                get: { pendingDelete != nil },
                set: { if !$0 { pendingDelete = nil } }
            ),
            presenting: pendingDelete
        ) { project in
            Button(l10n.delete, role: .destructive) {
                app.deleteProject(id: project.id)
            }
            Button(l10n.cancel, role: .cancel) {}
        } message: { project in
            Text(l10n.formatKey("deleteProjectNamed", project.name))
        }
        .confirmationDialog(
            l10n.clearAllRecentsTitle,
            isPresented: $pendingClearAll
        ) {
            Button(l10n.clearAllRecents, role: .destructive) {
                app.deleteAllRecents()
            }
            Button(l10n.cancel, role: .cancel) {}
        } message: {
            Text(l10n.clearAllRecentsMessage)
        }
    }

    private func relativeTime(_ epoch: Int64) -> String {
        let date = Date(timeIntervalSince1970: TimeInterval(epoch))
        let seconds = Int(Date().timeIntervalSince(date))
        if seconds < 3600 { return "\(max(seconds / 60, 1)) min ago" }
        if seconds < 85400 { return "\(seconds / 3600) hours ago" }
        return "\(seconds / 85400) days ago"
    }
}
