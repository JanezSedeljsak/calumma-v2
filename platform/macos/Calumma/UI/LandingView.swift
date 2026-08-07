import SwiftUI

struct LandingView: View {
    @EnvironmentObject private var app: AppModel
    @Environment(\.themeColors) private var colors
    @Environment(\.l10n) private var l10n
    @State private var name = ""
    @State private var width = 1280
    @State private var height = 720

    var body: some View {
        HStack(spacing: 0) {
            leftPane
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            heroPane
                .frame(width: 340)
        }
        .calmScreen()
        .toolbar {
            ToolbarItem(placement: .automatic) {
                CalmIconButton {
                    app.settingsOpen = true
                } icon: {
                    AppIcon.settings(color: colors.textMuted)
                }
            }
        }
        .onAppear {
            if name.isEmpty { name = l10n.newProject }
            app.engine.refreshRecents()
        }
    }

    private var leftPane: some View {
        VStack(alignment: .leading, spacing: Tokens.Space.xl) {
            HStack(alignment: .top) {
                VStack(alignment: .leading, spacing: Tokens.Space.sm) {
                    CalmText.brand(l10n.brand)
                    CalmText.eyebrow(l10n.tagline)
                }
                Spacer()
            }

            VStack(alignment: .leading, spacing: Tokens.Space.sm) {
                CalmText.label(l10n.projectName)
                CalmField(text: $name)
            }

            VStack(alignment: .leading, spacing: Tokens.Space.sm) {
                CalmText.label(l10n.resolution)
                HStack(spacing: Tokens.Space.sm) {
                    CalmNumberField(value: $width)
                    CalmNumberField(value: $height)
                    CalmAccentButton(title: l10n.create) {
                        app.create(name: name, width: width, height: height)
                    }
                }
            }

            HStack(alignment: .top, spacing: Tokens.Space.xl) {
                presetsColumn
                recentsColumn
            }
            .frame(maxHeight: .infinity, alignment: .top)
        }
        .padding(Tokens.Space.xxl)
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
                    .padding(Tokens.Space.md)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .calmSurface()
            } else {
                ForEach(app.engine.recents) { project in
                    CalmRowButton {
                        app.openRecent(project)
                    } content: {
                        CalmRow(
                            leading: {
                                CalmThumb(tint: recentTint(for: project))
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

    private var heroPane: some View {
        ZStack {
            LinearGradient(
                colors: [
                    colors.accentTeal.opacity(0.55),
                    colors.accentOrange.opacity(0.45),
                    colors.bg,
                ],
                startPoint: .topLeading,
                endPoint: .bottomTrailing
            )
            VStack(spacing: Tokens.Space.md) {
                AppIcon.image(color: .white)
                    .scaleEffect(1.4)
                Text(l10n.pasteArtwork)
                    .font(.system(size: Tokens.TypeSize.title, weight: .bold))
                    .foregroundStyle(.white)
            }
            .padding(Tokens.Space.xl)
            .background(.black.opacity(0.35), in: RoundedRectangle(cornerRadius: Tokens.Radius.md, style: .continuous))
        }
    }

    private func recentTint(for project: ProjectInfo) -> Color {
        let palette = [colors.accentTeal, colors.accentOrange, colors.danger]
        let idx = abs(project.id.hashValue) % palette.count
        return palette[idx]
    }

    private func relativeTime(_ epoch: Int64) -> String {
        let date = Date(timeIntervalSince1970: TimeInterval(epoch))
        let seconds = Int(Date().timeIntervalSince(date))
        if seconds < 3600 { return "\(max(seconds / 60, 1)) min ago" }
        if seconds < 86400 { return "\(seconds / 3600) hours ago" }
        return "\(seconds / 86400) days ago"
    }
}
