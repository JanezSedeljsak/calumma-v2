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
    var addLayer: String { self["addLayer"] }
    var showWorkspaces: String { self["showWorkspaces"] }
    var addWorkspace: String { self["addWorkspace"] }
    var deleteWorkspace: String { self["deleteWorkspace"] }
    var deleteProject: String { self["deleteProject"] }
    var layerVisibility: String { self["layerVisibility"] }
    var layerSettings: String { self["layerSettings"] }
    var deleteLayer: String { self["deleteLayer"] }
    var zoomIn: String { self["zoomIn"] }
    var zoomOut: String { self["zoomOut"] }
    var untitled: String { self["untitled"] }
    var untitledWorkspace: String { self["untitledWorkspace"] }
    var workspaces: String { self["workspaces"] }
    var workspaceName: String { self["workspaceName"] }
    var workspaceColor: String { self["workspaceColor"] }
    var newWorkspace: String { self["newWorkspace"] }
    var noWorkspaces: String { self["noWorkspaces"] }
    var deleteWorkspaceTitle: String { self["deleteWorkspaceTitle"] }
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
    var memoryUsed: String { self["memoryUsed"] }
    var vectorMode: String { self["vectorMode"] }
    var vectorModeHint: String { self["vectorModeHint"] }
    var shapes: String { self["shapes"] }
    var zoom: String { self["zoom"] }
    var ai: String { self["ai"] }
    var aiTools: String { self["aiTools"] }
    var removeBackground: String { self["removeBackground"] }
    var cutBackground: String { self["cutBackground"] }
    var removeBackgroundSuccess: String { self["removeBackgroundSuccess"] }
    var removeBackgroundFailed: String { self["removeBackgroundFailed"] }
    var removeBackgroundNeedsRaster: String { self["removeBackgroundNeedsRaster"] }
    var removeBackgroundWorking: String { self["removeBackgroundWorking"] }
    var undo: String { self["undo"] }
    var redo: String { self["redo"] }
    var themeLight: String { self["themeLight"] }
    var themeDark: String { self["themeDark"] }
    var boardMenu: String { self["boardMenu"] }
    var filtersMenu: String { self["filtersMenu"] }
    var enterFullScreen: String { self["enterFullScreen"] }
    var newProjectMenu: String { self["newProjectMenu"] }
    var fitToView: String { self["fitToView"] }
    var toggleLayers: String { self["toggleLayers"] }
    var toggleTheme: String { self["toggleTheme"] }
    var layerNamed: String { self["layerNamed"] }
    var paper: String { self["paper"] }
    var settings: String { self["settings"] }
    var version: String { self["version"] }
    var theme: String { self["theme"] }
    var language: String { self["language"] }
    var delete: String { self["delete"] }
    var cancel: String { self["cancel"] }
    var deleteProjectTitle: String { self["deleteProjectTitle"] }
    var clearAllRecents: String { self["clearAllRecents"] }
    var clearAllRecentsTitle: String { self["clearAllRecentsTitle"] }
    var clearAllRecentsMessage: String { self["clearAllRecentsMessage"] }
    var selectionTools: String { self["selectionTools"] }
    var exportMenu: String { self["exportMenu"] }
    var copyLayer: String { self["copyLayer"] }
    var canvasWidth: String { self["canvasWidth"] }
    var canvasHeight: String { self["canvasHeight"] }
    var duplicateLayer: String { self["duplicateLayer"] }
    var mergeLayerDown: String { self["mergeLayerDown"] }
    var opacity: String { self["opacity"] }
    var blendMode: String { self["blendMode"] }
    var blendNormal: String { self["blendNormal"] }
    var blendMultiply: String { self["blendMultiply"] }
    var blendScreen: String { self["blendScreen"] }
    var filters: String { self["filters"] }
    var brightness: String { self["brightness"] }
    var contrast: String { self["contrast"] }
    var vibrance: String { self["vibrance"] }
    var saturation: String { self["saturation"] }
    var levelsGamma: String { self["levelsGamma"] }
    var resetFilters: String { self["resetFilters"] }
    var resetTransform: String { self["resetTransform"] }
    var toolPen: String { self["toolPen"] }
    var toolEraser: String { self["toolEraser"] }
    var toolBucket: String { self["toolBucket"] }
    var toolEyedropper: String { self["toolEyedropper"] }
    var toolTransform: String { self["toolTransform"] }
    var toolText: String { self["toolText"] }
    var toolMove: String { self["toolMove"] }
    var textFont: String { self["textFont"] }
    var textSize: String { self["textSize"] }
    var textAlignLeft: String { self["textAlignLeft"] }
    var textAlignCenter: String { self["textAlignCenter"] }
    var textAlignRight: String { self["textAlignRight"] }
    var textNoFonts: String { self["textNoFonts"] }
    var textLineHeight: String { self["textLineHeight"] }
    var textBold: String { self["textBold"] }
    var textItalic: String { self["textItalic"] }
    var layerRasterizeText: String { self["layerRasterizeText"] }
    var exportLayer: String { self["exportLayer"] }
    var editText: String { self["editText"] }
    var toolLine: String { self["toolLine"] }
    var toolRect: String { self["toolRect"] }
    var toolEllipse: String { self["toolEllipse"] }
    var toolArrow: String { self["toolArrow"] }
    var toolTriangle: String { self["toolTriangle"] }
    var toolPentagon: String { self["toolPentagon"] }
    var toolSelectRect: String { self["toolSelectRect"] }
    var toolSelectEllipse: String { self["toolSelectEllipse"] }
    var toolSelectLasso: String { self["toolSelectLasso"] }
    var brushSize: String { self["brushSize"] }
    var inkOpacity: String { self["inkOpacity"] }
    var color: String { self["color"] }
    var primaryColor: String { self["primaryColor"] }
    var secondaryColor: String { self["secondaryColor"] }
    var hex: String { self["hex"] }

    func languageName(_ language: AppLanguage) -> String {
        self[language.displayKey]
    }
}
