import UniformTypeIdentifiers

enum ExportFormat: String, CaseIterable, Identifiable {
    case png
    case jpg
    case webp
    case avif
    case heic

    var id: String { rawValue }

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

    var ffiFormat: UInt32 {
        switch self {
        case .png: return 0
        case .jpg: return 1
        case .webp: return 2
        case .avif: return 3
        case .heic: return 4
        }
    }

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
