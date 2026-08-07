import SwiftUI

struct NewProjectView: View {
    var isLanding = true

    @EnvironmentObject private var app: AppModel
    @Environment(\.themeColors) private var colors
    @Environment(\.l10n) private var l10n
    @State private var name = ""
    @State private var width = 1280
    @State private var height = 720

    var body: some View {
        GeometryReader { geo in
            landscapeLayout(size: geo.size)
                .padding(Tokens.Space.lg)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        }
        .calmScreen()
        .background(ArtworkPasteCatcher(app: app))
        .toolbar {
            if isLanding {
                ToolbarItem(placement: .automatic) {
                    CalmIconButton {
                        app.settingsOpen = true
                    } icon: {
                        AppIcon.settings(color: colors.textMuted)
                    }
                }
            }
        }
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
                ScrollView(.vertical, showsIndicators: false) {
                    VStack(alignment: .leading, spacing: Tokens.Space.lg) {
                        createForm
                        HStack(alignment: .top, spacing: Tokens.Space.lg) {
                            presetsColumn
                            recentsColumn
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .topLeading)
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

    @ViewBuilder
    private var header: some View {
        if isLanding {
            VStack(alignment: .leading, spacing: Tokens.Space.xs) {
                CalmText.brand(l10n.brand)
                CalmText.eyebrow(l10n.tagline)
            }
        }
    }

    private var createForm: some View {
        VStack(alignment: .leading, spacing: Tokens.Space.lg) {
            VStack(alignment: .leading, spacing: Tokens.Space.xs) {
                CalmText.label(l10n.projectName)
                CalmField(text: $name)
            }

            VStack(alignment: .leading, spacing: Tokens.Space.xs) {
                CalmText.label(l10n.resolution)
                HStack(spacing: Tokens.Space.sm) {
                    CalmNumberField(value: $width, width: 72)
                    CalmNumberField(value: $height, width: 72)
                    CalmAccentButton(title: l10n.create) {
                        app.create(name: name, width: width, height: height)
                    }
                }
            }
        }
    }

    private var presetsColumn: some View {
        CalmSection(title: l10n.presets, accent: colors.accentTeal) {
            ForEach(Tokens.presets) { preset in
                CalmRowButton {
                    app.create(name: preset.label, width: preset.width, height: preset.height)
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

    private var recentsColumn: some View {
        CalmSection(title: l10n.recents, accent: colors.accentOrange) {
            if app.engine.recents.isEmpty {
                CalmText.muted(l10n.noRecents)
                    .padding(Tokens.Space.sm)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .calmSurface()
            } else {
                ForEach(app.engine.recents.prefix(5)) { project in
                    CalmRowButton {
                        app.openRecent(project)
                    } content: {
                        CalmRow(
                            leading: {
                                CalmThumb(tint: project.accentColor)
                                    .overlay(AppIcon.image(color: .white.opacity(0.9)))
                            },
                            title: project.name,
                            subtitle: "\(project.width) × \(project.height)",
                            trailing: relativeTime(project.openedAt)
                        )
                    }
                }
            }
        }
    }

    private func relativeTime(_ epoch: Int64) -> String {
        let date = Date(timeIntervalSince1970: TimeInterval(epoch))
        let seconds = Int(Date().timeIntervalSince(date))
        if seconds < 3600 { return "\(max(seconds / 60, 1)) min ago" }
        if seconds < 86400 { return "\(seconds / 3600) hours ago" }
        return "\(seconds / 86400) days ago"
    }
}
