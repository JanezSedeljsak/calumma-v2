import Foundation

enum AppLanguage: String, CaseIterable, Identifiable, Hashable {
    case en

    var id: String { rawValue }

    var resourceName: String { rawValue }

    var displayKey: String {
        switch self {
        case .en: return "languageEnglish"
        }
    }
}

struct L10nCatalog: Equatable {
    private let map: [String: String]

    static let fallback = L10nCatalog(map: [:])

    static func load(_ language: AppLanguage) -> L10nCatalog {
        if let url = Bundle.main.url(
            forResource: language.resourceName,
            withExtension: "json",
            subdirectory: "translations"
        ) ?? Bundle.main.url(forResource: language.resourceName, withExtension: "json")
        {
            return load(url: url)
        }
        let root = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
        let url = root
            .appendingPathComponent("translations", isDirectory: true)
            .appendingPathComponent("\(language.resourceName).json")
        if FileManager.default.fileExists(atPath: url.path) {
            return load(url: url)
        }
        return .fallback
    }

    private static func load(url: URL) -> L10nCatalog {
        guard let data = try? Data(contentsOf: url),
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: String]
        else {
            return .fallback
        }
        return L10nCatalog(map: json)
    }

    subscript(_ key: String) -> String {
        map[key] ?? key
    }

    func format(_ template: String, _ args: String...) -> String {
        Self.interpolate(template, args)
    }

    func formatKey(_ key: String, _ args: String...) -> String {
        Self.interpolate(self[key], args)
    }

    static func interpolate(_ template: String, _ args: [String]) -> String {
        var out = template
        for (index, arg) in args.enumerated() {
            out = out.replacingOccurrences(of: "{\(index)}", with: arg)
        }
        return out
    }

    var brand: String { self["brand"] }
    var tagline: String { self["tagline"] }
    var projectName: String { self["projectName"] }
    var projectColor: String { self["projectColor"] }
    var done: String { self["done"] }
    var resolution: String { self["resolution"] }
    var create: String { self["create"] }
    var newProject: String { self["newProject"] }
    var untitled: String { self["untitled"] }
    var presets: String { self["presets"] }
    var recents: String { self["recents"] }
    var noRecents: String { self["noRecents"] }
    var pasteArtwork: String { self["pasteArtwork"] }
    var pasteArtworkHint: String { self["pasteArtworkHint"] }
    var artworkFormats: String { self["artworkFormats"] }
    var chooseArtwork: String { self["chooseArtwork"] }
    var artworkImportFailed: String { self["artworkImportFailed"] }
    var layers: String { self["layers"] }
    var fill: String { self["fill"] }
    var shapes: String { self["shapes"] }
    var zoom: String { self["zoom"] }
    var ai: String { self["ai"] }
    var removeBackground: String { self["removeBackground"] }
    var cutBackground: String { self["cutBackground"] }
    var undo: String { self["undo"] }
    var redo: String { self["redo"] }
    var themeLight: String { self["themeLight"] }
    var themeDark: String { self["themeDark"] }
    var boardMenu: String { self["boardMenu"] }
    var newProjectMenu: String { self["newProjectMenu"] }
    var fitToView: String { self["fitToView"] }
    var toggleLayers: String { self["toggleLayers"] }
    var toggleTheme: String { self["toggleTheme"] }
    var layerNamed: String { self["layerNamed"] }
    var paper: String { self["paper"] }
    var settings: String { self["settings"] }
    var theme: String { self["theme"] }
    var language: String { self["language"] }
    var delete: String { self["delete"] }
    var cancel: String { self["cancel"] }
    var deleteProjectTitle: String { self["deleteProjectTitle"] }
    var selectionTools: String { self["selectionTools"] }
    var exportMenu: String { self["exportMenu"] }
    var copyLayer: String { self["copyLayer"] }

    func languageName(_ language: AppLanguage) -> String {
        self[language.displayKey]
    }
}
