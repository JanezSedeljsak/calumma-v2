import AppKit
import Foundation
import SwiftUI

enum CalmTool: UInt32 {
    case pen = 0
    case line = 1
    case rect = 2
    case ellipse = 3
    case arrow = 4

    var isShape: Bool {
        switch self {
        case .line, .rect, .ellipse, .arrow: return true
        case .pen: return false
        }
    }
}

struct ProjectInfo: Identifiable, Hashable {
    let id: String
    let name: String
    let width: Int
    let height: Int
    let openedAt: Int64
}

struct EngineState {
    var width: UInt32 = 0
    var height: UInt32 = 0
    var zoom: Float = 1
    var minZoom: Float = 1
    var maxZoom: Float = 1
    var panX: Float = 0
    var panY: Float = 0
    var activeLayer: UInt32 = 0
    var layerCount: UInt32 = 0
    var canUndo = false
    var canRedo = false
    var strokeActive = false
    var darkTheme = true
}

final class Engine: ObservableObject {
    private var ptr: OpaquePointer?
    @Published var state = EngineState()
    @Published var recents: [ProjectInfo] = []
    @Published var layerNames: [String] = []
    @Published var layerVisibles: [Bool] = []

    init() {
        ptr = calm_engine_new(nil)
        if let ptr {
            VisionPlatformOps.install(into: ptr)
        }
        refreshRecents()
    }

    deinit {
        if let ptr {
            calm_engine_free(ptr)
        }
    }

    var isReady: Bool { ptr != nil }

    func attach(layer: UnsafeMutableRawPointer, width: UInt32, height: UInt32, scale: Float) {
        guard let ptr else { return }
        _ = calm_engine_attach_surface(ptr, layer, width, height, scale)
    }

    func resize(width: UInt32, height: UInt32, scale: Float) {
        guard let ptr else { return }
        _ = calm_engine_resize(ptr, width, height, scale)
    }

    func render() {
        guard let ptr else { return }
        _ = calm_engine_render(ptr)
    }

    func pointerDown(x: Float, y: Float) {
        guard let ptr else { return }
        _ = calm_engine_pointer_down(ptr, x, y)
        syncState()
    }

    func pointerMove(x: Float, y: Float) {
        guard let ptr else { return }
        _ = calm_engine_pointer_move(ptr, x, y)
    }

    func pointerUp(x: Float, y: Float) {
        guard let ptr else { return }
        _ = calm_engine_pointer_up(ptr, x, y)
        syncState()
        refreshLayers()
    }

    func pan(dx: Float, dy: Float) {
        guard let ptr else { return }
        _ = calm_engine_pan(ptr, dx, dy)
        syncState()
    }

    func zoom(x: Float, y: Float, factor: Float) {
        guard let ptr else { return }
        _ = calm_engine_zoom(ptr, x, y, factor)
        syncState()
    }

    func fit() {
        guard let ptr else { return }
        _ = calm_engine_fit(ptr)
        syncState()
    }

    func setZoom(_ zoom: Float) {
        guard let ptr else { return }
        _ = calm_engine_set_zoom(ptr, zoom)
        syncState()
    }

    func setTool(_ tool: CalmTool) {
        guard let ptr else { return }
        _ = calm_engine_set_tool(ptr, tool.rawValue)
    }

    func setColor(_ color: Color) {
        guard let ptr else { return }
        let ns = NSColor(color).usingColorSpace(.sRGB) ?? NSColor.black
        var r: CGFloat = 0
        var g: CGFloat = 0
        var b: CGFloat = 0
        var a: CGFloat = 0
        ns.getRed(&r, green: &g, blue: &b, alpha: &a)
        _ = calm_engine_set_color(ptr, channel(r), channel(g), channel(b), channel(a))
    }

    private func channel(_ value: CGFloat) -> UInt8 {
        let scaled = (value * 255).rounded()
        guard scaled.isFinite else { return 0 }
        return UInt8(min(255, max(0, scaled)))
    }

    func setBrush(_ size: Float) {
        guard let ptr else { return }
        _ = calm_engine_set_brush(ptr, size)
    }

    func setFill(_ fill: Bool) {
        guard let ptr else { return }
        _ = calm_engine_set_fill(ptr, fill ? 1 : 0)
    }

    func setDark(_ dark: Bool) {
        guard let ptr else { return }
        _ = calm_engine_set_dark(ptr, dark ? 1 : 0)
        syncState()
    }

    func undo() {
        guard let ptr else { return }
        _ = calm_engine_undo(ptr)
        syncState()
        refreshLayers()
    }

    func redo() {
        guard let ptr else { return }
        _ = calm_engine_redo(ptr)
        syncState()
        refreshLayers()
    }

    func addLayer() {
        guard let ptr else { return }
        _ = calm_engine_add_layer(ptr)
        syncState()
        refreshLayers()
    }

    func removeLayer(_ index: Int) {
        guard let ptr else { return }
        _ = calm_engine_remove_layer(ptr, UInt32(index))
        syncState()
        refreshLayers()
        render()
    }

    func setLayerVisible(_ index: Int, visible: Bool) {
        guard let ptr else { return }
        _ = calm_engine_set_layer_visible(ptr, UInt32(index), visible ? 1 : 0)
        refreshLayers()
        render()
    }

    func setActiveLayer(_ index: Int) {
        guard let ptr else { return }
        _ = calm_engine_set_active_layer(ptr, UInt32(index))
        syncState()
    }

    func setHoverLayer(_ index: Int?) {
        guard let ptr else { return }
        _ = calm_engine_set_hover_layer(ptr, Int32(index ?? -1))
    }

    func clearLayer() {
        guard let ptr else { return }
        _ = calm_engine_clear_layer(ptr)
        syncState()
        refreshLayers()
    }

    @discardableResult
    func createProject(name: String, width: Int, height: Int) -> String? {
        guard let ptr else { return nil }
        let idPtr = name.withCString { calm_project_create(ptr, $0, UInt32(width), UInt32(height)) }
        guard let idPtr else { return nil }
        let id = String(cString: idPtr)
        calm_string_free(idPtr)
        syncState()
        refreshLayers()
        refreshRecents()
        return id
    }

    func openProject(id: String) {
        guard let ptr else { return }
        _ = id.withCString { calm_project_open(ptr, $0) }
        syncState()
        refreshLayers()
        refreshRecents()
    }

    func closeProject() {
        guard let ptr else { return }
        _ = calm_project_close(ptr)
        syncState()
        layerNames = []
        layerVisibles = []
    }

    func save() {
        guard let ptr else { return }
        _ = calm_project_save(ptr)
        refreshRecents()
    }

    func refreshRecents() {
        guard let ptr else { return }
        var buffer = Array(
            repeating: CalmProjectInfo(id: nil, name: nil, width: 0, height: 0, opened_at: 0),
            count: 32
        )
        let count = buffer.withUnsafeMutableBufferPointer { calm_project_list(ptr, $0.baseAddress, 32) }
        var items: [ProjectInfo] = []
        for i in 0..<count {
            let info = buffer[i]
            guard let idPtr = info.id, let namePtr = info.name else { continue }
            items.append(
                ProjectInfo(
                    id: String(cString: idPtr),
                    name: String(cString: namePtr),
                    width: Int(info.width),
                    height: Int(info.height),
                    openedAt: info.opened_at
                )
            )
            calm_string_free(idPtr)
            calm_string_free(namePtr)
        }
        recents = items
    }

    func syncState() {
        guard let ptr else { return }
        var raw = CalmState()
        _ = calm_engine_state(ptr, &raw)
        state = EngineState(
            width: raw.width,
            height: raw.height,
            zoom: raw.zoom,
            minZoom: raw.min_zoom,
            maxZoom: raw.max_zoom,
            panX: raw.pan_x,
            panY: raw.pan_y,
            activeLayer: raw.active_layer,
            layerCount: raw.layer_count,
            canUndo: raw.can_undo != 0,
            canRedo: raw.can_redo != 0,
            strokeActive: raw.stroke_active != 0,
            darkTheme: raw.dark_theme != 0
        )
    }

    func refreshLayers() {
        guard let ptr else { return }
        var names: [String] = []
        var visibles: [Bool] = []
        for i in 0..<state.layerCount {
            if let namePtr = calm_engine_layer_name(ptr, i) {
                let raw = String(cString: namePtr)
                calm_string_free(namePtr)
                names.append(raw == "Paper" ? L10nStore.catalog.paper : raw)
            } else {
                names.append(L10nStore.catalog.formatKey("layerNamed", "\(i + 1)"))
            }
            visibles.append(calm_engine_layer_visible(ptr, i) == 1)
        }
        layerNames = names
        layerVisibles = visibles
    }

    func layerThumbnail(index: Int, maxSide: UInt32 = 160) -> NSImage? {
        guard let ptr else { return nil }
        var rgbaPtr: UnsafeMutablePointer<UInt8>?
        var width: UInt32 = 0
        var height: UInt32 = 0
        let status = calm_engine_layer_thumbnail(
            ptr,
            UInt32(index),
            maxSide,
            &rgbaPtr,
            &width,
            &height
        )
        guard status == CalmStatusOk, let rgbaPtr, width > 0, height > 0 else { return nil }
        let byteCount = Int(width * height * 4)
        let data = Data(bytes: rgbaPtr, count: byteCount)
        calm_buffer_free(rgbaPtr, byteCount)
        let bitsPerComponent = 8
        let bytesPerRow = Int(width) * 4
        guard let provider = CGDataProvider(data: data as CFData),
              let cgImage = CGImage(
                  width: Int(width),
                  height: Int(height),
                  bitsPerComponent: bitsPerComponent,
                  bitsPerPixel: 32,
                  bytesPerRow: bytesPerRow,
                  space: CGColorSpaceCreateDeviceRGB(),
                  bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.last.rawValue),
                  provider: provider,
                  decode: nil,
                  shouldInterpolate: true,
                  intent: .defaultIntent
              )
        else {
            return nil
        }
        return NSImage(cgImage: cgImage, size: NSSize(width: Int(width), height: Int(height)))
    }

    var canRemoveBackground: Bool {
        guard let ptr else { return false }
        return calm_engine_op_available(ptr, UInt32(CalmOpKindRemoveBackground.rawValue))
    }

    func removeBackground() {
        guard let ptr else { return }
        let layer = state.activeLayer
        DispatchQueue.global(qos: .userInitiated).async {
            _ = calm_engine_run_op(ptr, UInt32(CalmOpKindRemoveBackground.rawValue), layer)
            DispatchQueue.main.async {
                self.syncState()
                self.refreshLayers()
                self.render()
            }
        }
    }
}
