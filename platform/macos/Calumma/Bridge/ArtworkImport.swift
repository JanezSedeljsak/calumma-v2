import AppKit
import SwiftUI
import UniformTypeIdentifiers

struct ArtworkImage {
    let name: String
    let encoded: Data
}

enum ArtworkImport {
    static let contentTypes: [UTType] = [
        .png,
        .jpeg,
        .webP,
        .heic,
        .svg,
        UTType("public.avif"),
        UTType("com.adobe.photoshop-image"),
    ].compactMap { $0 }

    static let fileExtensions: Set<String> = [
        "png", "jpg", "jpeg", "avif", "webp", "psd", "heic", "heif", "svg",
    ]

    static var pasteTypes: [UTType] { contentTypes + [.tiff] }

    static var dropTypes: [UTType] { [.fileURL] + pasteTypes }

    static func supports(_ url: URL) -> Bool {
        fileExtensions.contains(url.pathExtension.lowercased())
    }

    static func decode(url: URL) -> ArtworkImage? {
        guard supports(url), let encoded = try? Data(contentsOf: url), !encoded.isEmpty else {
            return nil
        }
        let name = url.deletingPathExtension().lastPathComponent
        return ArtworkImage(
            name: name.isEmpty ? L10nStore.catalog.untitled : name,
            encoded: encoded
        )
    }

    static func decode(data: Data, name: String) -> ArtworkImage? {
        guard !data.isEmpty else { return nil }
        return ArtworkImage(
            name: name.isEmpty ? L10nStore.catalog.untitled : name,
            encoded: data
        )
    }

    static func chooseFile(prompt: String, message: String) -> URL? {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = contentTypes
        panel.allowsMultipleSelection = false
        panel.canChooseDirectories = false
        panel.canCreateDirectories = false
        panel.prompt = prompt
        panel.message = message
        return panel.runModal() == .OK ? panel.url : nil
    }

    static func fromPasteboard(_ pasteboard: NSPasteboard = .general) -> ArtworkImage? {
        fromPasteboardAll(pasteboard).first
    }

    static func fromPasteboardAll(_ pasteboard: NSPasteboard = .general) -> [ArtworkImage] {
        if let urls = pasteboard.readObjects(forClasses: [NSURL.self]) as? [URL] {
            let artworks = urls.compactMap { url -> ArtworkImage? in
                supports(url) ? decode(url: url) : nil
            }
            if !artworks.isEmpty { return artworks }
        }
        for type in pasteTypes {
            let raw = NSPasteboard.PasteboardType(type.identifier)
            if let data = pasteboard.data(forType: raw),
               let artwork = decode(data: data, name: L10nStore.catalog.untitled)
            {
                return [artwork]
            }
        }
        return []
    }

    static func load(
        providers: [NSItemProvider],
        into complete: @MainActor @escaping (ArtworkImage?) -> Void
    ) -> Bool {
        loadAll(providers: providers) { artworks in
            complete(artworks.first)
        }
    }

    static func loadAll(
        providers: [NSItemProvider],
        into complete: @MainActor @escaping ([ArtworkImage]) -> Void
    ) -> Bool {
        let urlProviders = providers.enumerated().filter { $0.element.canLoadObject(ofClass: URL.self) }
        if !urlProviders.isEmpty {
            let group = DispatchGroup()
            var results: [(Int, ArtworkImage)] = []
            let lock = NSLock()
            for (index, provider) in urlProviders {
                group.enter()
                _ = provider.loadObject(ofClass: URL.self) { url, _ in
                    defer { group.leave() }
                    guard let url, let artwork = decode(url: url) else { return }
                    lock.lock()
                    results.append((index, artwork))
                    lock.unlock()
                }
            }
            group.notify(queue: .main) {
                let sorted = results.sorted(by: { $0.0 < $1.0 }).map(\.1)
                Task { @MainActor in
                    complete(sorted)
                }
            }
            return true
        }
        let dataProviders: [(Int, NSItemProvider, UTType)] = providers.enumerated().compactMap {
            index, provider in
            guard let type = pasteTypes.first(where: {
                provider.hasItemConformingToTypeIdentifier($0.identifier)
            }) else { return nil }
            return (index, provider, type)
        }
        if !dataProviders.isEmpty {
            let group = DispatchGroup()
            var results: [(Int, ArtworkImage)] = []
            let lock = NSLock()
            let fallbackName = L10nStore.catalog.untitled
            for (index, provider, type) in dataProviders {
                group.enter()
                provider.loadDataRepresentation(forTypeIdentifier: type.identifier) { data, _ in
                    defer { group.leave() }
                    guard let data, let artwork = decode(data: data, name: fallbackName) else { return }
                    lock.lock()
                    results.append((index, artwork))
                    lock.unlock()
                }
            }
            group.notify(queue: .main) {
                let sorted = results.sorted(by: { $0.0 < $1.0 }).map(\.1)
                Task { @MainActor in
                    complete(sorted)
                }
            }
            return true
        }
        return false
    }
}
