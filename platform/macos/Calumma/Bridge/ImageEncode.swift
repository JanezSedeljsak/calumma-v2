import ImageIO
import UniformTypeIdentifiers

enum ExportFormat: String, CaseIterable, Identifiable {
    case png
    case jpg
    case webp
    case avif

    var id: String { rawValue }

    var utType: UTType {
        switch self {
        case .png: return .png
        case .jpg: return .jpeg
        case .webp: return .webP
        case .avif: return UTType("public.avif") ?? .png
        }
    }

    var fileExtension: String { rawValue }

    var label: String {
        switch self {
        case .png: return "PNG"
        case .jpg: return "JPEG"
        case .webp: return "WebP"
        case .avif: return "AVIF"
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
        if format == .jpg || format == .webp {
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
