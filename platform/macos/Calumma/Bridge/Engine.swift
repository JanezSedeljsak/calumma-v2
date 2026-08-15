import AppKit
import Foundation
import QuartzCore
import SwiftUI

enum CalmBlendMode: UInt32, CaseIterable, Identifiable {
    case normal = 0
    case multiply = 1
    case screen = 2

    var id: UInt32 { rawValue }
}

enum CalmAdjustment: UInt32, CaseIterable, Identifiable {
    case brightness = 0
    case contrast = 1
    case vibrance = 2
    case saturation = 3
    case levelsGamma = 4

    var id: UInt32 { rawValue }

    var labelKey: String {
        switch self {
        case .brightness: return "brightness"
        case .contrast: return "contrast"
        case .vibrance: return "vibrance"
        case .saturation: return "saturation"
        case .levelsGamma: return "levelsGamma"
        }
    }

    var shortcutKey: KeyEquivalent {
        switch self {
        case .brightness: return "b"
        case .contrast: return "c"
        case .vibrance: return "v"
        case .saturation: return "s"
        case .levelsGamma: return "g"
        }
    }
}

struct LayerAdjustments: Equatable {
    var brightness: Float = 0
    var contrast: Float = 0
    var vibrance: Float = 0
    var saturation: Float = 0
    var levelsGamma: Float = 1

    var isNeutral: Bool {
        self == LayerAdjustments()
    }
}

enum CalmTool: UInt32 {
    case pen = 0
    case line = 1
    case rect = 2
    case ellipse = 3
    case arrow = 4
    case eraser = 5
    case selectRect = 6
    case selectEllipse = 7
    case selectLasso = 8
    case bucket = 9
    case eyedropper = 11
    case triangle = 12
    case pentagon = 13
    case text = 14
    case move = 15

    var isShape: Bool { calm_tool_is_shape(rawValue) != 0 }
    var isSelection: Bool { calm_tool_is_selection(rawValue) != 0 }
    var takesBrushSize: Bool { calm_tool_takes_brush_size(rawValue) != 0 }
    var takesInkOpacity: Bool { calm_tool_takes_ink_opacity(rawValue) != 0 }
    var showsVectorMode: Bool { calm_tool_shows_vector_mode(rawValue) != 0 }
}

struct ProjectInfo: Identifiable, Hashable {
    let id: String
    let name: String
    let width: Int
    let height: Int
    let openedAt: Int64
    var accent: UInt32 = 0

    var accentColor: Color { Color(rgb: accent) }
}

struct WorkspaceInfo: Identifiable, Hashable {
    let id: String
    var name: String
    var accent: UInt32
    var activeProjectId: String?
    let openedAt: Int64

    var accentColor: Color { Color(rgb: accent) }
}

extension Color {
    init(rgb: UInt32) {
        self.init(
            red: Double((rgb >> 16) & 0xFF) / 255,
            green: Double((rgb >> 8) & 0xFF) / 255,
            blue: Double(rgb & 0xFF) / 255
        )
    }

    var packedRGBA: UInt32 {
        let ns = NSColor(self).usingColorSpace(.sRGB) ?? NSColor.black
        var r: CGFloat = 0
        var g: CGFloat = 0
        var b: CGFloat = 0
        var a: CGFloat = 0
        ns.getRed(&r, green: &g, blue: &b, alpha: &a)
        let byte = { (value: CGFloat) -> UInt32 in
            UInt32(min(255, max(0, (value * 255).rounded())))
        }
        return (byte(r) << 24) | (byte(g) << 16) | (byte(b) << 8) | byte(a)
    }

    var packedRGB: UInt32 { packedRGBA >> 8 }
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
    var accent: UInt32 = 0
    var zoomUnit: Float = 0
    var lastShapeTool: CalmTool = .rect
    var lastSelectTool: CalmTool = .selectRect

    var accentColor: Color { Color(rgb: accent) }
}

final class Engine: ObservableObject, @unchecked Sendable {
    /// Readable across the bridge's own files (`EngineText`), settable only here.
    private(set) var ptr: OpaquePointer?
    @Published var state = EngineState()
    @Published var recents: [ProjectInfo] = []
    @Published var workspaces: [WorkspaceInfo] = []
    @Published var layerNames: [String] = []
    @Published var layerVisibles: [Bool] = []
    @Published var layerOpacities: [Float] = []
    @Published var layerBlendModes: [CalmBlendMode] = []
    @Published var layerAdjustments: [LayerAdjustments] = []
    @Published var layerIsText: [Bool] = []
    /// Mirrors of engine-owned text state. The shell never computes any of this — it shows
    /// what `syncTextState` last read back, so a font substituted or a size clamped by the
    /// engine is what the panel displays.
    @Published var textEditing = false
    @Published var textFamily = ""
    @Published var textSize: Float = 48
    @Published var textAlign: CalmTextAlign = .left
    @Published var textLineHeight: Float = 1.25
    @Published var textBold = false
    @Published var textItalic = false
    @Published private(set) var thumbnailRevision: UInt64 = 0
    /// The layer an AI op is currently running against, so the layers panel can show it's
    /// busy — `nil` the rest of the time, including right after the op finishes.
    @Published private(set) var aiOpBusyLayer: Int?

    /// A fit asked for while the window is still growing to its final size lands against
    /// the viewport the board *had*, which is how a freshly opened project ends up
    /// off-centre. `fitToScreen` keeps the request open for this long so every resize that
    /// arrives in the meantime re-fits, and the board settles on the size the user sees.
    private static let fitGraceSeconds: CFTimeInterval = 0.8
    private var fitDeadline: CFTimeInterval = 0
    private var stateDirty = false

    static var inkOpacityMin: Float { calm_ink_opacity_min() }
    static var inkOpacityMax: Float { calm_ink_opacity_max() }
    static var inkOpacityDefault: Float { calm_ink_opacity_default() }

    init() {
        ptr = calm_engine_new(nil)
        if let ptr {
            VisionPlatformOps.install(into: ptr)
        }
        refreshRecents()
        refreshWorkspaces()
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
        if CACurrentMediaTime() < fitDeadline {
            _ = calm_engine_fit(ptr)
        }
        // The zoom bounds are viewport-relative, so the slider is stale until this lands.
        syncStateSoon()
    }

    func resizeDocument(width: Int, height: Int) {
        guard let ptr else { return }
        _ = calm_engine_resize_document(ptr, UInt32(width), UInt32(height))
        syncState()
        refreshLayers()
        render()
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
        fitDeadline = 0
        _ = calm_engine_pan(ptr, dx, dy)
        syncStateSoon()
    }

    func panScroll(dx: Float, dy: Float, precise: Bool) {
        guard let ptr else { return }
        fitDeadline = 0
        _ = calm_engine_pan_scroll(ptr, dx, dy, precise ? 1 : 0)
        syncStateSoon()
    }

    func zoom(x: Float, y: Float, factor: Float) {
        guard let ptr else { return }
        fitDeadline = 0
        _ = calm_engine_zoom(ptr, x, y, factor)
        syncStateSoon()
    }

    func zoomScroll(x: Float, y: Float, delta: Float, precise: Bool) {
        guard let ptr else { return }
        fitDeadline = 0
        _ = calm_engine_zoom_scroll(ptr, x, y, delta, precise ? 1 : 0)
        syncStateSoon()
    }

    func fit() {
        guard let ptr else { return }
        fitDeadline = 0
        _ = calm_engine_fit(ptr)
        syncState()
    }

    /// Fit now *and* on every resize for the next moment — see `fitGraceSeconds`. Use this
    /// wherever the board is shown for the first time (opening a project, attaching the
    /// surface); `fit()` is the plain one for a user asking to fit right now.
    func fitToScreen() {
        guard let ptr else { return }
        _ = calm_engine_fit(ptr)
        fitDeadline = CACurrentMediaTime() + Self.fitGraceSeconds
        syncState()
    }

    func setZoom(_ zoom: Float) {
        guard let ptr else { return }
        _ = calm_engine_set_zoom(ptr, zoom)
        syncState()
    }

    func stepZoom(in zoomIn: Bool) {
        guard let ptr else { return }
        _ = calm_engine_step_zoom(ptr, zoomIn ? 1 : 0)
        syncState()
    }

    func setZoomUnit(_ unit: Float) {
        guard let ptr else { return }
        _ = calm_engine_set_zoom_unit(ptr, unit)
        syncState()
    }

    func setBoardColors(desk: Color, grid: Color, paperBorder: Color) {
        guard let ptr else { return }
        _ = calm_engine_set_board_colors(
            ptr,
            desk.packedRGBA,
            grid.packedRGBA,
            paperBorder.packedRGBA
        )
    }

    static var palette: [Color] {
        (0..<calm_palette_count()).map { Color(rgb: calm_palette_color($0)) }
    }

    func rename(projectId: String, to name: String) {
        guard let ptr else { return }
        _ = projectId.withCString { idPtr in
            name.withCString { namePtr in
                calm_project_rename(ptr, idPtr, namePtr)
            }
        }
        syncState()
        refreshRecents()
    }

    func setAccent(projectId: String, color: Color) {
        guard let ptr else { return }
        _ = projectId.withCString { calm_project_set_accent(ptr, $0, color.packedRGB) }
        syncState()
        refreshRecents()
    }

    func deleteProject(id: String) {
        guard let ptr else { return }
        _ = id.withCString { calm_project_delete(ptr, $0) }
        syncState()
        refreshRecents()
    }

    func deleteAllProjects() {
        guard let ptr else { return }
        _ = calm_project_delete_all(ptr)
        syncState()
        refreshRecents()
        refreshWorkspaces()
    }

    func setTool(_ tool: CalmTool) {
        guard let ptr else { return }
        _ = calm_engine_set_tool(ptr, tool.rawValue)
        syncState()
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

    func sampleColor(x: Float, y: Float) -> Color? {
        guard let ptr else { return nil }
        var packed: UInt32 = 0
        guard calm_engine_sample_color(ptr, x, y, &packed) == CalmStatusOk else {
            return nil
        }
        return Self.color(fromPackedRGBA: packed)
    }

    func pickColor(x: Float, y: Float) -> Color? {
        guard let ptr else { return nil }
        var packed: UInt32 = 0
        guard calm_engine_pick_color(ptr, x, y, &packed) == CalmStatusOk else {
            return nil
        }
        return Self.color(fromPackedRGBA: packed)
    }

    private static func color(fromPackedRGBA packed: UInt32) -> Color {
        let r = Double((packed >> 24) & 0xFF) / 255
        let g = Double((packed >> 16) & 0xFF) / 255
        let b = Double((packed >> 8) & 0xFF) / 255
        let a = Double(packed & 0xFF) / 255
        return Color(.sRGB, red: r, green: g, blue: b, opacity: a)
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

    func setInkOpacity(_ opacity: Float) {
        guard let ptr else { return }
        _ = calm_engine_set_ink_opacity(ptr, opacity)
    }

    func setFill(_ fill: Bool) {
        guard let ptr else { return }
        _ = calm_engine_set_fill(ptr, fill ? 1 : 0)
    }

    func setVectorMode(_ on: Bool) {
        guard let ptr else { return }
        _ = calm_engine_set_vector_mode(ptr, on ? 1 : 0)
    }

    var vectorMode: Bool {
        guard let ptr else { return false }
        return calm_engine_vector_mode(ptr) != 0
    }

    func setDark(_ dark: Bool) {
        guard let ptr else { return }
        _ = calm_engine_set_dark(ptr, dark ? 1 : 0)
        syncState()
    }

    func setShift(_ held: Bool) {
        guard let ptr else { return }
        _ = calm_engine_set_shift(ptr, held ? 1 : 0)
    }

    func resetLayerTransform(_ index: Int) {
        guard let ptr else { return }
        _ = calm_engine_reset_layer_transform(ptr, UInt32(index))
        render()
    }

    func toggleTransform() {
        guard let ptr else { return }
        _ = calm_engine_toggle_transform(ptr)
        render()
        syncState()
    }

    func exitTransform() {
        guard let ptr else { return }
        _ = calm_engine_exit_transform(ptr)
        render()
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

    func duplicateLayer(_ index: Int) {
        guard let ptr else { return }
        _ = calm_engine_duplicate_layer(ptr, UInt32(index))
        syncState()
        refreshLayers()
        render()
    }

    func moveLayerUp(_ index: Int) {
        guard let ptr else { return }
        _ = calm_engine_move_layer_up(ptr, UInt32(index))
        syncState()
        refreshLayers()
        render()
    }

    func moveLayerDown(_ index: Int) {
        guard let ptr else { return }
        _ = calm_engine_move_layer_down(ptr, UInt32(index))
        syncState()
        refreshLayers()
        render()
    }

    func mergeLayerDown(_ index: Int) {
        guard let ptr else { return }
        _ = calm_engine_merge_layer_down(ptr, UInt32(index))
        syncState()
        refreshLayers()
        render()
    }

    func setLayerOpacity(_ index: Int, _ opacity: Float) {
        guard let ptr else { return }
        _ = calm_engine_set_layer_opacity(ptr, UInt32(index), opacity)
        refreshLayers()
        render()
    }

    func setLayerBlendMode(_ index: Int, _ mode: CalmBlendMode) {
        guard let ptr else { return }
        _ = calm_engine_set_layer_blend_mode(ptr, UInt32(index), mode.rawValue)
        refreshLayers()
        render()
    }

    func setLayerAdjustments(_ index: Int, _ adjustments: LayerAdjustments) {
        guard let ptr else { return }
        _ = calm_engine_set_layer_adjustments(
            ptr,
            UInt32(index),
            adjustments.brightness,
            adjustments.contrast,
            adjustments.vibrance,
            adjustments.saturation,
            adjustments.levelsGamma
        )
        refreshLayers()
        render()
    }

    func nudgeLayerAdjustment(_ index: Int, _ kind: CalmAdjustment, steps: Float) {
        guard let ptr else { return }
        _ = calm_engine_nudge_layer_adjustment(ptr, UInt32(index), kind.rawValue, steps)
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

    static var importMaxSide: Int { Int(calm_import_max_side()) }

    @discardableResult
    func createProject(name: String, artwork: ArtworkImage) -> String? {
        guard let ptr else { return nil }
        let created: String? = artwork.premultipliedRGBA.withUnsafeBytes { raw in
            guard let base = raw.bindMemory(to: UInt8.self).baseAddress else { return nil }
            let idPtr = name.withCString {
                calm_project_create_from_image(
                    ptr,
                    $0,
                    UInt32(artwork.width),
                    UInt32(artwork.height),
                    base,
                    raw.count
                )
            }
            guard let idPtr else { return nil }
            let id = String(cString: idPtr)
            calm_string_free(idPtr)
            return id
        }
        guard let created else { return nil }
        syncState()
        refreshLayers()
        refreshRecents()
        return created
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
        layerOpacities = []
        layerBlendModes = []
        layerAdjustments = []
        layerIsText = []
        textEditing = false
    }

    func save() {
        guard let ptr else { return }
        _ = calm_project_save(ptr)
        bumpThumbnailRevision()
        refreshRecents()
    }

    /// Bytes the engine is holding for the project that is open — everything else was handed
    /// back when its document was closed, so this is the whole picture.
    var memoryBytes: UInt64 {
        guard let ptr else { return 0 }
        var out = CalmMemory()
        guard calm_engine_memory(ptr, &out) == CalmStatusOk else { return 0 }
        return out.tile_bytes + out.history_bytes + out.mask_bytes + out.vector_bytes
            + out.text_bytes + out.gpu_bytes
    }

    func bumpThumbnailRevision() {
        thumbnailRevision &+= 1
    }

    func projectThumbnailPNG(projectId: String) -> Data? {
        guard let ptr else { return nil }
        var out: UnsafeMutablePointer<UInt8>?
        var len: Int = 0
        let status = projectId.withCString { calm_project_thumbnail(ptr, $0, &out, &len) }
        guard status == CalmStatusOk, let out, len > 0 else { return nil }
        let data = Data(bytes: out, count: len)
        calm_buffer_free(out, len)
        return data
    }

    func refreshWorkspaces() {
        guard let ptr else { return }
        var buffer = Array(
            repeating: CalmWorkspaceInfo(
                id: nil,
                name: nil,
                accent: 0,
                active_project_id: nil,
                opened_at: 0
            ),
            count: 64
        )
        let count = buffer.withUnsafeMutableBufferPointer {
            calm_workspace_list(ptr, $0.baseAddress, 64)
        }
        var items: [WorkspaceInfo] = []
        for i in 0..<count {
            let info = buffer[i]
            guard let idPtr = info.id, let namePtr = info.name else { continue }
            let active = info.active_project_id.map { String(cString: $0) }
            items.append(
                WorkspaceInfo(
                    id: String(cString: idPtr),
                    name: String(cString: namePtr),
                    accent: info.accent,
                    activeProjectId: active,
                    openedAt: info.opened_at
                )
            )
            calm_string_free(idPtr)
            calm_string_free(namePtr)
            if let activePtr = info.active_project_id {
                calm_string_free(activePtr)
            }
        }
        workspaces = items
    }

    @discardableResult
    func createWorkspace(name: String) -> String? {
        guard let ptr else { return nil }
        let idPtr = name.withCString { calm_workspace_create(ptr, $0) }
        guard let idPtr else { return nil }
        let id = String(cString: idPtr)
        calm_string_free(idPtr)
        refreshWorkspaces()
        return id
    }

    @discardableResult
    func createWorkspaceForProject(projectId: String, name: String) -> String? {
        guard let ptr else { return nil }
        let idPtr = projectId.withCString { projectPtr in
            name.withCString { namePtr in
                calm_workspace_create_for_project(ptr, projectPtr, namePtr)
            }
        }
        guard let idPtr else { return nil }
        let id = String(cString: idPtr)
        calm_string_free(idPtr)
        refreshWorkspaces()
        return id
    }

    func workspaceForProject(projectId: String) -> String? {
        guard let ptr else { return nil }
        let idPtr = projectId.withCString { calm_workspace_for_project(ptr, $0) }
        guard let idPtr else { return nil }
        let id = String(cString: idPtr)
        calm_string_free(idPtr)
        return id
    }

    func renameWorkspace(id: String, to name: String) {
        guard let ptr else { return }
        _ = id.withCString { idPtr in
            name.withCString { namePtr in
                calm_workspace_rename(ptr, idPtr, namePtr)
            }
        }
        refreshWorkspaces()
    }

    func setWorkspaceAccent(id: String, color: Color) {
        guard let ptr else { return }
        _ = id.withCString { calm_workspace_set_accent(ptr, $0, color.packedRGB) }
        refreshWorkspaces()
    }

    func deleteWorkspace(id: String) {
        guard let ptr else { return }
        _ = id.withCString { calm_workspace_delete(ptr, $0) }
        refreshWorkspaces()
    }

    func addProjectToWorkspace(workspaceId: String, projectId: String) {
        guard let ptr else { return }
        _ = workspaceId.withCString { wsPtr in
            projectId.withCString { projectPtr in
                calm_workspace_add_project(ptr, wsPtr, projectPtr)
            }
        }
        refreshWorkspaces()
    }

    func removeProjectFromWorkspace(workspaceId: String, projectId: String) {
        guard let ptr else { return }
        _ = workspaceId.withCString { wsPtr in
            projectId.withCString { projectPtr in
                calm_workspace_remove_project(ptr, wsPtr, projectPtr)
            }
        }
        refreshWorkspaces()
    }

    func workspaceProjects(workspaceId: String) -> [ProjectInfo] {
        guard let ptr else { return [] }
        var buffer = Array(
            repeating: CalmProjectInfo(
                id: nil,
                name: nil,
                width: 0,
                height: 0,
                opened_at: 0,
                accent: 0
            ),
            count: 32
        )
        let count = workspaceId.withCString { wsPtr in
            buffer.withUnsafeMutableBufferPointer {
                calm_workspace_projects(ptr, wsPtr, $0.baseAddress, 32)
            }
        }
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
                    openedAt: info.opened_at,
                    accent: info.accent
                )
            )
            calm_string_free(idPtr)
            calm_string_free(namePtr)
        }
        return items
    }

    func setWorkspaceActiveProject(workspaceId: String, projectId: String?) {
        guard let ptr else { return }
        _ = workspaceId.withCString { wsPtr in
            if let projectId {
                projectId.withCString { calm_workspace_set_active_project(ptr, wsPtr, $0) }
            } else {
                calm_workspace_set_active_project(ptr, wsPtr, nil)
            }
        }
        refreshWorkspaces()
    }

    func touchWorkspace(id: String) {
        guard let ptr else { return }
        _ = id.withCString { calm_workspace_touch(ptr, $0) }
        refreshWorkspaces()
    }

    func workspace(id: String) -> WorkspaceInfo? {
        guard let ptr else { return nil }
        var info = CalmWorkspaceInfo(
            id: nil,
            name: nil,
            accent: 0,
            active_project_id: nil,
            opened_at: 0
        )
        let status = id.withCString { calm_workspace_get(ptr, $0, &info) }
        guard status == CalmStatusOk, let idPtr = info.id, let namePtr = info.name else {
            return nil
        }
        let active = info.active_project_id.map { String(cString: $0) }
        let item = WorkspaceInfo(
            id: String(cString: idPtr),
            name: String(cString: namePtr),
            accent: info.accent,
            activeProjectId: active,
            openedAt: info.opened_at
        )
        calm_string_free(idPtr)
        calm_string_free(namePtr)
        if let activePtr = info.active_project_id {
            calm_string_free(activePtr)
        }
        return item
    }

    func loadOpenWorkspaceTabs() -> [String] {
        guard let ptr else { return [] }
        var buffer = Array(repeating: Optional<UnsafeMutablePointer<CChar>>.none, count: 64)
        let count = buffer.withUnsafeMutableBufferPointer {
            calm_open_workspace_tabs(ptr, $0.baseAddress, 64)
        }
        var ids: [String] = []
        for i in 0..<count {
            guard let idPtr = buffer[i] else { continue }
            ids.append(String(cString: idPtr))
            calm_string_free(idPtr)
        }
        return ids
    }

    func persistOpenWorkspaceTabs(_ ids: [String]) {
        guard let ptr else { return }
        if ids.isEmpty {
            _ = calm_set_open_workspace_tabs(ptr, nil, 0)
            return
        }
        var owned = ids.map { strdup($0) }
        defer {
            for item in owned {
                if let item { free(item) }
            }
        }
        owned.withUnsafeMutableBufferPointer { buffer in
            buffer.baseAddress?.withMemoryRebound(
                to: UnsafePointer<CChar>?.self,
                capacity: ids.count
            ) { rebound in
                _ = calm_set_open_workspace_tabs(ptr, rebound, ids.count)
            }
        }
    }

    func refreshRecents() {
        guard let ptr else { return }
        var buffer = Array(
            repeating: CalmProjectInfo(
                id: nil,
                name: nil,
                width: 0,
                height: 0,
                opened_at: 0,
                accent: 0
            ),
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
                    openedAt: info.opened_at,
                    accent: info.accent
                )
            )
            calm_string_free(idPtr)
            calm_string_free(namePtr)
        }
        recents = items
    }

    /// Pan and zoom events arrive faster than the display refreshes, and every published
    /// state drags the whole SwiftUI editor through a diff. Mark the state stale instead
    /// and let the canvas flush it once per frame.
    func syncStateSoon() {
        stateDirty = true
    }

    /// Called from the render loop, so the UI still tracks the camera — just at frame rate.
    func flushPendingState() {
        guard stateDirty else { return }
        syncState()
    }

    func syncState() {
        stateDirty = false
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
            darkTheme: raw.dark_theme != 0,
            accent: raw.accent,
            zoomUnit: raw.zoom_unit,
            lastShapeTool: CalmTool(rawValue: raw.last_shape_tool) ?? .rect,
            lastSelectTool: CalmTool(rawValue: raw.last_select_tool) ?? .selectRect
        )
    }

    func refreshLayers() {
        guard let ptr else { return }
        var names: [String] = []
        var visibles: [Bool] = []
        var opacities: [Float] = []
        var blendModes: [CalmBlendMode] = []
        var adjustments: [LayerAdjustments] = []
        var isText: [Bool] = []
        for i in 0..<state.layerCount {
            if let namePtr = calm_engine_layer_name(ptr, i) {
                let raw = String(cString: namePtr)
                calm_string_free(namePtr)
                names.append(raw == "Paper" ? L10nStore.catalog.paper : raw)
            } else {
                names.append(L10nStore.catalog.formatKey("layerNamed", "\(i + 1)"))
            }
            visibles.append(calm_engine_layer_visible(ptr, i) == 1)
            isText.append(calm_engine_layer_is_text(ptr, i) == 1)
            opacities.append(calm_engine_layer_opacity(ptr, i))
            blendModes.append(CalmBlendMode(rawValue: calm_engine_layer_blend_mode(ptr, i)) ?? .normal)
            var raw = CalmAdjustments()
            _ = calm_engine_layer_adjustments(ptr, i, &raw)
            adjustments.append(
                LayerAdjustments(
                    brightness: raw.brightness,
                    contrast: raw.contrast,
                    vibrance: raw.vibrance,
                    saturation: raw.saturation,
                    levelsGamma: raw.levels_gamma
                )
            )
        }
        layerNames = names
        layerVisibles = visibles
        layerOpacities = opacities
        layerBlendModes = blendModes
        layerAdjustments = adjustments
        layerIsText = isText
        syncTextState()
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

    private static func cgImage(
        rgbaPtr: UnsafeMutablePointer<UInt8>?,
        width: UInt32,
        height: UInt32,
        status: CalmStatus
    ) -> CGImage? {
        guard status == CalmStatusOk, let rgbaPtr, width > 0, height > 0 else { return nil }
        let byteCount = Int(width * height * 4)
        let data = Data(bytes: rgbaPtr, count: byteCount)
        calm_buffer_free(rgbaPtr, byteCount)
        guard let provider = CGDataProvider(data: data as CFData) else { return nil }
        return CGImage(
            width: Int(width),
            height: Int(height),
            bitsPerComponent: 8,
            bitsPerPixel: 32,
            bytesPerRow: Int(width) * 4,
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.last.rawValue),
            provider: provider,
            decode: nil,
            shouldInterpolate: true,
            intent: .defaultIntent
        )
    }

    func exportPSD() -> Data? {
        guard let ptr else { return nil }
        var bytesPtr: UnsafeMutablePointer<UInt8>?
        var len: Int = 0
        let status = calm_engine_export_psd(ptr, &bytesPtr, &len)
        guard status == CalmStatusOk, let bytesPtr, len > 0 else { return nil }
        let data = Data(bytes: bytesPtr, count: len)
        calm_buffer_free(bytesPtr, len)
        return data
    }

    func compositeCGImage() -> CGImage? {
        guard let ptr else { return nil }
        var rgbaPtr: UnsafeMutablePointer<UInt8>?
        var width: UInt32 = 0
        var height: UInt32 = 0
        let status = calm_engine_composite_rgba(ptr, &rgbaPtr, &width, &height)
        return Self.cgImage(rgbaPtr: rgbaPtr, width: width, height: height, status: status)
    }

    func layerCGImage(index: Int) -> CGImage? {
        guard let ptr else { return nil }
        var rgbaPtr: UnsafeMutablePointer<UInt8>?
        var width: UInt32 = 0
        var height: UInt32 = 0
        let status = calm_engine_layer_rgba(ptr, UInt32(index), &rgbaPtr, &width, &height)
        return Self.cgImage(rgbaPtr: rgbaPtr, width: width, height: height, status: status)
    }

    func selectionCGImage() -> CGImage? {
        guard let ptr else { return nil }
        var rgbaPtr: UnsafeMutablePointer<UInt8>?
        var width: UInt32 = 0
        var height: UInt32 = 0
        let status = calm_engine_selection_rgba(ptr, &rgbaPtr, &width, &height)
        return Self.cgImage(rgbaPtr: rgbaPtr, width: width, height: height, status: status)
    }

    /// The whole document as one SVG. Layered on purpose: vector layers stay geometry, and
    /// only the layers that really are pixels are embedded as images.
    func exportSVG() -> String? {
        guard let ptr, let cStr = calm_engine_export_svg(ptr) else { return nil }
        let svg = String(cString: cStr)
        calm_string_free(cStr)
        return svg
    }

    func layerSVG(index: Int) -> String? {
        guard let ptr, let cStr = calm_engine_layer_svg(ptr, UInt32(index)) else { return nil }
        let svg = String(cString: cStr)
        calm_string_free(cStr)
        return svg
    }

    func isLayerVector(index: Int) -> Bool {
        guard let ptr else { return false }
        return calm_engine_layer_is_vector(ptr, UInt32(index)) == 1
    }

    func layerItemCount(index: Int) -> Int {
        guard let ptr else { return 0 }
        return Int(calm_engine_layer_item_count(ptr, UInt32(index)))
    }

    /// The vector item being moved — `nil` when nothing is selected. Its layer is always
    /// `state.activeLayer`, so there is nothing else to ask for.
    var selectedVectorItem: Int? {
        guard let ptr else { return nil }
        let item = calm_engine_selected_vector_item(ptr)
        return item >= 0 ? Int(item) : nil
    }

    func clearVectorSelection() {
        guard let ptr else { return }
        _ = calm_engine_clear_vector_selection(ptr)
        render()
    }

    func deleteSelectedVectorItem() {
        guard let ptr else { return }
        _ = calm_engine_delete_selected_vector_item(ptr)
        render()
        refreshLayers()
    }

    func nudgeSelectedVectorItem(x: Float, y: Float) {
        guard let ptr else { return }
        _ = calm_engine_nudge_selected_vector_item(ptr, x, y)
        render()
    }

    var hasSelection: Bool {
        guard let ptr else { return false }
        return calm_engine_has_selection(ptr) != 0
    }

    func deselect() {
        guard let ptr else { return }
        _ = calm_engine_deselect(ptr)
        render()
    }

    func clearSelectionPixels() {
        guard let ptr else { return }
        _ = calm_engine_selection_clear_pixels(ptr)
        syncState()
        render()
    }

    func pasteImage(premultipliedRGBA: Data, width: Int, height: Int) {
        guard let ptr else { return }
        premultipliedRGBA.withUnsafeBytes { raw in
            guard let base = raw.bindMemory(to: UInt8.self).baseAddress else { return }
            _ = calm_engine_paste_image(
                ptr, base, premultipliedRGBA.count, UInt32(width), UInt32(height)
            )
        }
        syncState()
        refreshLayers()
        render()
    }

    var canRemoveBackground: Bool {
        guard let ptr else { return false }
        return calm_engine_op_available(ptr, UInt32(CalmOpKindRemoveBackground.rawValue))
            && aiOpBusyLayer == nil
    }

    /// Runs Vision's foreground-mask op on the active layer and reports what actually
    /// happened via `onFinished` — this used to discard the `CalmStatus` from
    /// `calm_engine_run_op` entirely, so a failure (Vision finding no foreground subject,
    /// most likely, which is plausible on hand-drawn art rather than a photo) looked
    /// identical to success: nothing happened, silently. `Engine` has no strings to show for
    /// any of this itself — `AppModel`, which owns `l10n`, turns the result into a toast —
    /// so this only reports which of the three outcomes occurred. The layer is marked busy
    /// for as long as Vision is actually running, so a slow request reads as "working," not
    /// "broken."
    func removeBackground(onFinished: @escaping (AiOpResult) -> Void) {
        guard let ptr, aiOpBusyLayer == nil else { return }
        let layerIndex = Int(state.activeLayer)
        guard !isLayerVector(index: layerIndex) else {
            onFinished(.ineligibleLayer)
            return
        }
        let layer = state.activeLayer
        aiOpBusyLayer = layerIndex
        DispatchQueue.global(qos: .userInitiated).async {
            let status = calm_engine_run_op(ptr, UInt32(CalmOpKindRemoveBackground.rawValue), layer)
            DispatchQueue.main.async {
                self.aiOpBusyLayer = nil
                self.syncState()
                self.refreshLayers()
                self.render()
                onFinished(status == CalmStatusOk ? .success : .failed)
            }
        }
    }
}

/// What a Platform AI op (Remove Background, today) actually did — `Engine` reports this
/// instead of a pre-localized message, since it has no access to `l10n`.
enum AiOpResult {
    case success
    case failed
    case ineligibleLayer
}
