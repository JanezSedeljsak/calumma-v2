import Foundation
import SwiftUI

struct SettingsView: View {
    @EnvironmentObject private var app: AppModel
    @Environment(\.l10n) private var l10n
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(alignment: .leading, spacing: Tokens.Space.xl) {
            HStack {
                CalmText.title(l10n.settings, strong: true)
                Spacer()
                Button {
                    dismiss()
                } label: {
                    Text("×")
                        .foregroundStyle(app.colors.textMuted)
                }
                .buttonStyle(.plain)
                .calmPointer()
            }

            VStack(alignment: .leading, spacing: Tokens.Space.sm) {
                CalmText.label(l10n.theme)
                HStack(spacing: Tokens.Space.sm) {
                    themeChip(.light, title: l10n.themeLight)
                    themeChip(.dark, title: l10n.themeDark)
                }
            }

            VStack(alignment: .leading, spacing: Tokens.Space.sm) {
                CalmText.label(l10n.language)
                ForEach(AppLanguage.allCases) { language in
                    Button {
                        app.setLanguage(language)
                    } label: {
                        HStack {
                            CalmText.body(l10n.languageName(language), strong: app.language == language)
                            Spacer()
                        }
                        .padding(Tokens.Space.md)
                        .calmSurface(hover: app.language == language)
                    }
                    .buttonStyle(.plain)
                    .calmPointer()
                }
            }

            HStack {
                CalmText.label(l10n.memoryUsed)
                Spacer()
                CalmText.muted(memoryLabel, mono: true)
            }

            Spacer()
        }
        .padding(Tokens.Space.xl)
        .frame(width: 360, height: 360)
        .calmScreen()
    }

    /// The engine reports bytes; the shell only formats them.
    private var memoryLabel: String {
        ByteCountFormatter.string(
            fromByteCount: Int64(app.engine.memoryBytes),
            countStyle: .memory
        )
    }

    private func themeChip(_ theme: AppTheme, title: String) -> some View {
        Button {
            app.setTheme(theme)
        } label: {
            Text(title)
                .font(.system(size: Tokens.TypeSize.body, weight: app.theme == theme ? .bold : .medium))
                .foregroundStyle(app.theme == theme ? app.colors.text : app.colors.textMuted)
                .padding(.horizontal, Tokens.Space.lg)
                .padding(.vertical, Tokens.Space.md)
                .calmSurface(hover: app.theme == theme)
        }
        .buttonStyle(.plain)
        .calmPointer()
    }
}
