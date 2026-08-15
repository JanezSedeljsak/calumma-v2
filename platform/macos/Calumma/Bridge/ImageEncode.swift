import ImageIO
import UniformTypeIdentifiers

enum ExportFormat: String, CaseIterable, Identifiable {
    case png
    case jpg
    case webp
    case avif
    case heic

    var id: String { rawValue }

    /// The format a chosen filename asks for, `jpeg` included since the save panel writes the
    /// type's preferred extension and ours is `jpg`.
    init?(fileExtension: String) {
        let ext = fileExtension.lowercased()
        guard let match = ExportFormat.allCases.first(where: {
            $0.rawValue == ext || $0.utType.preferredFilenameExtension == ext
        }) else {
            return nil
        }
        self = match
    }

    var utType: UTType {
        switch self {
        case .png: return .png
        case .jpg: return .jpeg
        case .webp: return .webP
        case .avif: return UTType("public.avif") ?? .png
        case .heic: return .heic
        }
    }

    var fileExtension: String { rawValue }

    /// Formats that take a quality knob. AVIF and HEIC are lossy too, but their encoders read
    /// the same destination option, so they all go through one branch.
    var isLossy: Bool { self != .png }

    var label: String {
        switch self {
        case .png: return "PNG"
        case .jpg: return "JPEG"
        case .webp: return "WebP"
        case .avif: return "AVIF"
        case .heic: return "HEIC"
        }
    }
}

enum ImageEncode {
    static func data(_ image: CGImage, format: ExportFormat, quality: CGFloat = 0.92) -> Data? {
        let out = NSMutableData()
        guard let destination = CGImageDestinationCreateWithData(
            out, format.utType.identifier as CFString, 1, nil
        ) else {
            return nil
        }
        var options: [CFString: Any] = [:]
        if format.isLossy {
            options[kCGImageDestinationLossyCompressionQuality] = quality
        }
        CGImageDestinationAddImage(destination, image, options as CFDictionary)
        guard CGImageDestinationFinalize(destination) else { return nil }
        return out as Data
    }

    static func pngData(_ image: CGImage) -> Data? {
        data(image, format: .png)
    }
}
