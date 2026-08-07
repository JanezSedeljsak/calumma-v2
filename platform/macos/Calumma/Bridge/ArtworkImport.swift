import AppKit
import ImageIO
import SwiftUI
import UniformTypeIdentifiers

struct ArtworkImage {
    let name: String
    let width: Int
    let height: Int
    let premultipliedRGBA: Data
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
        guard let source = CGImageSourceCreateWithURL(url as CFURL, nil) else { return nil }
        let name = url.deletingPathExtension().lastPathComponent
        return decode(source: source, name: name, allowing: contentTypes)
    }

    static func decode(data: Data, name: String) -> ArtworkImage? {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil) else { return nil }
        return decode(source: source, name: name, allowing: pasteTypes)
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
        if let urls = pasteboard.readObjects(forClasses: [NSURL.self]) as? [URL],
           let url = urls.first(where: supports),
           let artwork = decode(url: url)
        {
            return artwork
        }
        for type in pasteTypes {
            let raw = NSPasteboard.PasteboardType(type.identifier)
            if let data = pasteboard.data(forType: raw),
               let artwork = decode(data: data, name: L10nStore.catalog.untitled)
            {
                return artwork
            }
        }
        return nil
    }

    static func load(
        providers: [NSItemProvider],
        into complete: @MainActor @escaping (ArtworkImage?) -> Void
    ) -> Bool {
        for provider in providers where provider.canLoadObject(ofClass: URL.self) {
            _ = provider.loadObject(ofClass: URL.self) { url, _ in
                let artwork = url.flatMap(decode(url:))
                Task { @MainActor in complete(artwork) }
            }
            return true
        }
        for provider in providers {
            guard let type = pasteTypes.first(where: {
                provider.hasItemConformingToTypeIdentifier($0.identifier)
            }) else { continue }
            let fallbackName = L10nStore.catalog.untitled
            provider.loadDataRepresentation(forTypeIdentifier: type.identifier) { data, _ in
                let artwork = data.flatMap { decode(data: $0, name: fallbackName) }
                Task { @MainActor in complete(artwork) }
            }
            return true
        }
        return false
    }

    private static func decode(
        source: CGImageSource,
        name: String,
        allowing: [UTType]
    ) -> ArtworkImage? {
        guard let uti = CGImageSourceGetType(source) as String?,
              let kind = UTType(uti),
              allowing.contains(where: kind.conforms(to:))
        else {
            return nil
        }
        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: Engine.importMaxSide,
            kCGImageSourceShouldCacheImmediately: true,
        ]
        guard let image = CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary)
        else {
            return nil
        }
        return rasterize(image, name: name)
    }

    private static func rasterize(_ image: CGImage, name: String) -> ArtworkImage? {
        let width = image.width
        let height = image.height
        guard width > 0, height > 0, width <= Engine.importMaxSide, height <= Engine.importMaxSide
        else {
            return nil
        }
        var bytes = [UInt8](repeating: 0, count: width * height * 4)
        guard let space = CGColorSpace(name: CGColorSpace.sRGB) else { return nil }
        let drew: Bool = bytes.withUnsafeMutableBytes { raw in
            guard let context = CGContext(
                data: raw.baseAddress,
                width: width,
                height: height,
                bitsPerComponent: 8,
                bytesPerRow: width * 4,
                space: space,
                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
            ) else {
                return false
            }
            context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
            return true
        }
        guard drew else { return nil }
        return ArtworkImage(
            name: name.isEmpty ? L10nStore.catalog.untitled : name,
            width: width,
            height: height,
            premultipliedRGBA: Data(bytes)
        )
    }
}
