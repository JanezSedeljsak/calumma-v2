import SwiftUI
import UniformTypeIdentifiers

struct PasteArtworkIsland: View {
    @EnvironmentObject private var app: AppModel
    @Environment(\.themeColors) private var colors
    @Environment(\.l10n) private var l10n
    @State private var targeted = false

    var body: some View {
        CalmIsland(padding: 0) {
            GeometryReader { geo in
                ZStack {
                    LinearGradient(
                        colors: [
                            colors.accentTeal.opacity(targeted ? 0.7 : 0.45),
                            colors.accentOrange.opacity(targeted ? 0.6 : 0.35),
                            colors.surface,
                        ],
                        startPoint: .topLeading,
                        endPoint: .bottomTrailing
                    )
                    caption(roomy: geo.size.height >= Tokens.Window.pasteMinHeight)
                        .padding(Tokens.Space.md)
                }
                .frame(width: geo.size.width, height: geo.size.height)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .contentShape(Rectangle())
            .clipShape(RoundedRectangle(cornerRadius: Tokens.Radius.island, style: .continuous))
        }
        .calmPointer()
        .onTapGesture { app.chooseArtwork() }
        .onDrop(of: ArtworkImport.dropTypes, isTargeted: $targeted) { providers in
            app.dropArtwork(providers: providers)
        }
    }

    private func caption(roomy: Bool) -> some View {
        VStack(spacing: Tokens.Space.sm) {
            AppIcon.image(color: colors.text)
                .scaleEffect(1.2)
            Text(l10n.pasteArtwork)
                .font(.system(size: Tokens.TypeSize.body, weight: .bold))
                .foregroundStyle(colors.text)
            if roomy {
                Text(l10n.pasteArtworkHint)
                    .font(.system(size: Tokens.TypeSize.label))
                    .foregroundStyle(colors.text.opacity(0.7))
                Text(l10n.artworkFormats)
                    .font(.system(size: Tokens.TypeSize.label, weight: .medium))
                    .foregroundStyle(colors.text.opacity(0.5))
            }
            if let error = app.artworkError {
                Text(error)
                    .font(.system(size: Tokens.TypeSize.label, weight: .semibold))
                    .foregroundStyle(colors.danger)
            }
        }
        .multilineTextAlignment(.center)
        .minimumScaleFactor(0.7)
    }
}

struct ArtworkPasteCatcher: NSViewRepresentable {
    let app: AppModel

    func makeNSView(context: Context) -> NSView {
        let view = PasteView()
        view.app = app
        DispatchQueue.main.async {
            guard let window = view.window, window.firstResponder === window else { return }
            window.makeFirstResponder(view)
        }
        return view
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        (nsView as? PasteView)?.app = app
    }

    final class PasteView: NSView {
        nonisolated(unsafe) weak var app: AppModel?

        override var acceptsFirstResponder: Bool { true }

        @objc func paste(_ sender: Any?) {
            MainActor.assumeIsolated {
                app?.pasteArtwork()
            }
        }
    }
}
