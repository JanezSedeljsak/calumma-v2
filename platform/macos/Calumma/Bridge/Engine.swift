import AppKit
import Foundation
import QuartzCore
import SwiftUI

/// What a paste actually did. The shell never works this out by comparing sizes — the engine
/// decided, so the engine reports.
enum CalmPasteOutcome: UInt32 {
    case failed = 0
    case native = 1
    /// The image was bigger than the paper, so the layer holds more than the canvas shows.
    case overflowing = 2
}

enum CalmBlendMode: UInt32, CaseIterable, Identifiable {
    case normal = 0
    case multiply = 1
    case screen = 2

    var id: UInt32 { rawValue }
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

enum CalmBrush: UInt32, CaseIterable {
    case pen = 0
    case marker = 1
    case crayon = 2
    case airbrush = 3
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
    case transform = 10
    case eyedropper = 11
    case triangle = 12
    case pentagon = 13
    case text = 14
    case move = 15
    case blur = 16
    case magicWand = 17

    var isShape: Bool { calm_tool_is_shape(rawValue) != 0 }
    var isSelection: Bool { calm_tool_is_selection(rawValue) != 0 }
    var takesFill: Bool { calm_tool_takes_fill(rawValue) != 0 }
    var takesBrushSize: Bool { calm_tool_takes_brush_size(rawValue) != 0 }
    var takesInkOpacity: Bool { calm_tool_takes_ink_opacity(rawValue) != 0 }
    var showsVectorMode: Bool { calm_tool_shows_vector_mode(rawValue) != 0 }
    var takesBlurStrength: Bool { calm_tool_takes_blur_strength(rawValue) != 0 }
    var takesTolerance: Bool { calm_tool_takes_tolerance(rawValue) != 0 }
    var takesEyedropperRadius: Bool { calm_tool_takes_eyedropper_radius(rawValue) != 0 }
    var takesBrush: Bool { calm_tool_takes_brush(rawValue) != 0 }
    var takesEraserHardness: Bool { calm_tool_takes_eraser_hardness(rawValue) != 0 }
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
    /// Whether the board already shows what Fit to View would show. The engine answers it —
    /// the shell never recomputes a fit — and the zoom pill's Fit button lights up on it.
    var isFit = false
    /// Whether the active layer is inside `⌘T`. Transform is a mode the engine owns; Move's
    /// options toggle and the `⌘T` shortcut both read this rather than `AppModel.tool`.
    var transformActive = false

    var accentColor: Color { Color(rgb: accent) }
}

/// One ruler mark, in document pixels — the engine already resolved the adaptive
/// 1/2/5×10ⁿ spacing, so the shell only draws what it's given.
struct LayerThumbnailEntry {
    var revision: UInt64
    var row: NSImage?
    var card: NSImage?
}

struct RulerTick {
    var doc: Float
    var major: Bool
}

final class Engine: ObservableObject, @unchecked Sendable {
    /// Readable across the bridge's own files (`EngineText`), settable only here.
    private(set) var ptr: OpaquePointer?
    @Published var state = EngineState()
    @Published var recents: [ProjectInfo] = []
    /// Bumped on every change to the guide list — added, moved, flipped, removed. The count
    /// alone cannot stand in for this: two of those four leave it untouched.
    @Published private(set) var guidesRevision: UInt64 = 0
    /// Where a guide being dragged currently sits. Deliberately *not* `@Published` on the
    /// engine: it changes on every pointer move of the drag, and an engine publish re-renders
    /// every view observing `AppModel` — the whole editor, tools and layers included. It gets
    /// its own small observable so the readout label is the only thing that redraws.
    let guideReadout = GuideReadoutStore()
    @Published var layerNames: [String] = []
    @Published var layerVisibles: [Bool] = []
    @Published var layerOpacities: [Float] = []
    @Published var layerBlendModes: [CalmBlendMode] = []
    @Published var layerAdjustments: [LayerAdjustments] = []
    @Published var layerIsText: [Bool] = []
    /// Why each tool cannot run on the active layer, indexed by `CalmTool.rawValue` — the
    /// engine's whole rule table, read once per sync. See `EngineToolGate.swift`.
    @Published var toolBlocks: [CalmToolBlock] = []
    /// Set when the active layer pins vector mode on, so the toggle shows itself as decided
    /// rather than as a knob that is quietly ignored.
    @Published var vectorModeLocked = false
    /// The reason the last board press did nothing. Whoever says it out loud clears it.
    @Published var toolBlockNotice: CalmToolBlock = .none
    @Published var layerLocked: [Bool] = []
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
    /// One rendered preview per layer, parallel to `layerNames`. Rows read this array — they
    /// never call into the engine while building their body, which is what made a click on the
    /// eye take seconds on a deep stack: six `@Published` writes per refresh, each re-running
    /// every row, each row rasterising a layer.
    @Published private(set) var layerThumbnails: [NSImage?] = []
    /// Memo behind `layerThumbnails`, keyed by layer id and that layer's content revision. A
    /// preview is a function of its own layer's pixels and nothing else, so anything that does
    /// not touch pixels — visibility, opacity, blend mode, reordering, edits to *other* layers
    /// — finds the same entry and reuses the exact same image.
    private var layerThumbnailMemo: [String: LayerThumbnailEntry] = [:]
    /// Full-size previews, for the hover card. Only one is ever on screen, so these are never
    /// the per-row cost that `layerThumbnails` is.
    @Published private(set) var layerPreviewCards: [NSImage?] = []
    /// The layer an AI op is currently running against, so the layers panel can show it's
    /// busy — `nil` the rest of the time, including right after the op finishes.
    @Published private(set) var aiOpBusyLayer: Int?
    /// How many guides the open document holds. Only a count, because that is all the chrome
    /// needs — the guides themselves are drawn by the board, never by SwiftUI. See
    /// `EngineGuides.swift`.
    @Published private(set) var guideCount = 0

    /// A fit asked for while the window is still growing to its final size lands against
    /// the viewport the board *had*, which is how a freshly opened project ends up
    /// off-centre. `fitToScreen` keeps the request open for this long so every resize that
    /// arrives in the meantime re-fits, and the board settles on the size the user sees.
    private static let fitGraceSeconds: CFTimeInterval = 0.8
    private var fitDeadline: CFTimeInterval = 0
    private var stateDirty = false
    /// Owns the OS memory-pressure subscription — see `EngineMemoryPressure.swift`. Kept here
    /// only because the source has to outlive the closure that reads it; every other detail of
    /// what it does lives in that file, the way `guideReadout` and `EngineGuides.swift` split.
    var memoryPressureSource: DispatchSourceMemoryPressure?

    static var brushSizeMin: Float { calm_brush_size_min() }
    static var brushSizeMax: Float { calm_brush_size_max() }
    static var brushSizeDefault: Float { calm_brush_size_default() }
    /// Brush and text size sliders run on 0...1 of travel and let the engine place the value
    /// on its curve — the same arrangement the zoom pill has with `zoom_unit`, and for the
    /// same reason: the curve is a product decision, not a piece of panel layout.
    static func brushSizeUnit(_ size: Float) -> Float { calm_brush_size_unit(size) }
    static func brushSize(fromUnit unit: Float) -> Float { calm_brush_size_from_unit(unit) }
    static func brushSizeStep(_ size: Float, increase: Bool) -> Float {
        calm_brush_size_step(size, increase ? 1 : 0)
    }
    static var inkOpacityMin: Float { calm_ink_opacity_min() }
    static var inkOpacityMax: Float { calm_ink_opacity_max() }
    static var inkOpacityDefault: Float { calm_ink_opacity_default() }
    static var blurStrengthMin: Float { calm_blur_strength_min() }
    static var blurStrengthMax: Float { calm_blur_strength_max() }
    static var blurStrengthDefault: Float { calm_blur_strength_default() }
    static var eraserHardnessMin: Float { calm_eraser_hardness_min() }
    static var eraserHardnessMax: Float { calm_eraser_hardness_max() }
    static var eraserHardnessDefault: Float { calm_eraser_hardness_default() }
    static var toleranceMin: UInt8 { calm_tolerance_min() }
    static var toleranceMax: UInt8 { calm_tolerance_max() }
    static var toleranceDefault: UInt8 { calm_tolerance_default() }
    static var eyedropperRadiusMin: UInt32 { calm_eyedropper_radius_min() }
    static var eyedropperRadiusMax: UInt32 { calm_eyedropper_radius_max() }
    static var eyedropperRadiusDefault: UInt32 { calm_eyedropper_radius_default() }

    init() {
        ptr = calm_engine_new(nil)
        if let ptr {
            VisionPlatformOps.install(into: ptr)
        }
        refreshRecents()
        startObservingMemoryPressure()
    }

    deinit {
        memoryPressureSource?.cancel()
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
        // A press on the board can grab a guide as well as pull one off a ruler, so the readout
        // has to start here too.
        refreshGuideReadout()
    }

    func pointerMove(x: Float, y: Float) {
        guard let ptr else { return }
        _ = calm_engine_pointer_move(ptr, x, y)
        // The one thing a move publishes. Gated on a drag already being in flight so an ordinary
        // stroke — where this is the hot path — pays nothing for it.
        if guideReadout.readout != nil {
            refreshGuideReadout()
        }
    }

    func pointerUp(x: Float, y: Float) {
        guard let ptr else { return }
        _ = calm_engine_pointer_up(ptr, x, y)
        syncState()
        refreshLayers()
        refreshGuideReadout()
    }

    func pan(dx: Float, dy: Float) {
        guard let ptr else { return }
        fitDeadline = 0
        _ = calm_engine_pan(ptr, dx, dy)
        syncStateSoon()
    }

    func endCameraMotion() {
        guard let ptr else { return }
        _ = calm_engine_end_camera_motion(ptr)
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

    /// Where a document of that size lands once fitted, in viewport points — the engine's own
    /// fit geometry, asked without an open project. The canvas placeholder shown while a
    /// project loads is drawn on it, so it sits exactly where the paper is about to.
    static func fitSize(viewport: CGSize, document: CGSize) -> CGSize {
        var width: Float = 0
        var height: Float = 0
        guard calm_fit_size(
            Float(viewport.width),
            Float(viewport.height),
            Float(document.width),
            Float(document.height),
            &width,
            &height
        ) == CalmStatusOk else {
            return .zero
        }
        return CGSize(width: CGFloat(width), height: CGFloat(height))
    }

    struct FitCamera {
        var zoom: Float
        var panX: Float
        var panY: Float
    }

    /// The camera a fit would leave behind, asked without an open project. Rulers use it
    /// while a project is still loading so their ticks match the board that is about to land.
    static func fitCamera(viewport: CGSize, document: CGSize) -> FitCamera? {
        var zoom: Float = 0
        var panX: Float = 0
        var panY: Float = 0
        guard calm_fit_camera(
            Float(viewport.width),
            Float(viewport.height),
            Float(document.width),
            Float(document.height),
            &zoom,
            &panX,
            &panY
        ) == CalmStatusOk, zoom > 0 else {
            return nil
        }
        return FitCamera(zoom: zoom, panX: panX, panY: panY)
    }

    /// The board viewport the engine last resized to — still valid while a project loads.
    var boardViewport: CGSize? {
        guard let ptr else { return nil }
        var width: Float = 0
        var height: Float = 0
        guard calm_engine_viewport(ptr, &width, &height) == CalmStatusOk, width > 0, height > 0 else {
            return nil
        }
        return CGSize(width: CGFloat(width), height: CGFloat(height))
    }

    /// The desk's squared paper, in screen points — the same table `board.wgsl` lays the real
    /// grid on, so the loading placeholder can put its own on the same lattice.
    static let desk: DeskMetrics = {
        var raw = CalmDeskMetrics(
            cell: 26,
            line_width: 1,
            cross_arm: 3.5,
            cross_line_width: 1.1,
            line_alpha: 0.4
        )
        guard calm_desk_metrics(&raw) == CalmStatusOk else { return DeskMetrics(raw) }
        return DeskMetrics(raw)
    }()

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

    /// Adaptive tick positions for one ruler axis, in document pixels — recomputed fresh
    /// each call rather than cached, since `RulerView` only asks while the camera state it
    /// reads has actually changed. `cap` is generous headroom over what any real viewport
    /// produces (`RULER_MIN_MINOR_SPACING_PX` bounds the count to roughly `viewport / 8`).
    private static let rulerTickCapacity = 1024

    func rulerTicksX() -> [RulerTick] {
        rulerTicks { calm_engine_ruler_ticks_x($0, $1, $2) }
    }

    func rulerTicksY() -> [RulerTick] {
        rulerTicks { calm_engine_ruler_ticks_y($0, $1, $2) }
    }

    static func rulerTicksX(zoom: Float, pan: Float, viewportExtent: Float) -> [RulerTick] {
        rulerTicks(zoom: zoom, pan: pan, viewportExtent: viewportExtent) {
            calm_ruler_ticks_x(zoom, pan, viewportExtent, $0, $1)
        }
    }

    static func rulerTicksY(zoom: Float, pan: Float, viewportExtent: Float) -> [RulerTick] {
        rulerTicks(zoom: zoom, pan: pan, viewportExtent: viewportExtent) {
            calm_ruler_ticks_y(zoom, pan, viewportExtent, $0, $1)
        }
    }

    private func rulerTicks(
        _ call: (OpaquePointer, UnsafeMutablePointer<CalmRulerTick>?, Int) -> Int
    ) -> [RulerTick] {
        guard let ptr else { return [] }
        var buffer = Array(
            repeating: CalmRulerTick(doc: 0, major: 0),
            count: Self.rulerTickCapacity
        )
        let count = buffer.withUnsafeMutableBufferPointer { call(ptr, $0.baseAddress, Self.rulerTickCapacity) }
        return (0..<count).map { RulerTick(doc: buffer[$0].doc, major: buffer[$0].major != 0) }
    }

    private static func rulerTicks(
        zoom: Float,
        pan: Float,
        viewportExtent: Float,
        _ call: (UnsafeMutablePointer<CalmRulerTick>?, Int) -> Int
    ) -> [RulerTick] {
        var buffer = Array(
            repeating: CalmRulerTick(doc: 0, major: 0),
            count: rulerTickCapacity
        )
        let count = buffer.withUnsafeMutableBufferPointer {
            call($0.baseAddress, rulerTickCapacity)
        }
        return (0..<count).map { RulerTick(doc: buffer[$0].doc, major: buffer[$0].major != 0) }
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
    }

    func setTool(_ tool: CalmTool) {
        guard let ptr else { return }
        _ = calm_engine_set_tool(ptr, tool.rawValue)
        syncState()
    }

    func setColor(_ color: Color) {
        guard let ptr else { return }
        let (r, g, b, a) = channels(color)
        _ = calm_engine_set_color(ptr, r, g, b, a)
    }

    func setStrokeColor(_ color: Color) {
        guard let ptr else { return }
        let (r, g, b, a) = channels(color)
        _ = calm_engine_set_stroke_color(ptr, r, g, b, a)
    }

    func setShapeFillColor(_ color: Color) {
        guard let ptr else { return }
        let (r, g, b, a) = channels(color)
        _ = calm_engine_set_shape_fill_color(ptr, r, g, b, a)
    }

    private func channels(_ color: Color) -> (UInt8, UInt8, UInt8, UInt8) {
        let ns = NSColor(color).usingColorSpace(.sRGB) ?? NSColor.black
        var r: CGFloat = 0
        var g: CGFloat = 0
        var b: CGFloat = 0
        var a: CGFloat = 0
        ns.getRed(&r, green: &g, blue: &b, alpha: &a)
        return (channel(r), channel(g), channel(b), channel(a))
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

    func setBlurStrength(_ strength: Float) {
        guard let ptr else { return }
        _ = calm_engine_set_blur_strength(ptr, strength)
    }

    func setTolerance(_ tolerance: UInt8) {
        guard let ptr else { return }
        _ = calm_engine_set_tolerance(ptr, tolerance)
    }

    func setEyedropperRadius(_ radius: UInt32) {
        guard let ptr else { return }
        _ = calm_engine_set_eyedropper_radius(ptr, radius)
    }

    func setBrush(_ brush: CalmBrush) {
        guard let ptr else { return }
        _ = calm_engine_set_brush_kind(ptr, brush.rawValue)
    }

    func setEraserHardness(_ hardness: Float) {
        guard let ptr else { return }
        _ = calm_engine_set_eraser_hardness(ptr, hardness)
    }

    func setFill(_ fill: Bool) {
        guard let ptr else { return }
        _ = calm_engine_set_fill(ptr, fill ? 1 : 0)
    }

    func setStroke(_ stroke: Bool) {
        guard let ptr else { return }
        _ = calm_engine_set_stroke(ptr, stroke ? 1 : 0)
    }

    func setVectorMode(_ on: Bool) {
        guard let ptr else { return }
        _ = calm_engine_set_vector_mode(ptr, on ? 1 : 0)
        // The one knob that changes which tools the active layer will take, so the panel has
        // to hear about it here rather than waiting for the next state sync.
        syncToolGate()
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

    /// The pointer while no button is down, so the board can draw the brush where it is. No
    /// `render()` — the canvas is already drawing every frame, and this only marks the overlay
    /// stale for whichever frame comes next.
    func setPointerHover(x: Float, y: Float) {
        guard let ptr else { return }
        _ = calm_engine_set_pointer_hover(ptr, x, y)
    }

    func clearPointerHover() {
        guard let ptr else { return }
        _ = calm_engine_clear_pointer_hover(ptr)
    }

    /// Whether the board is ringing the pointer right now. Asked on every cursor refresh so the
    /// shell can stand its own cursor down while the ring is the pointer — a plain read, no
    /// publish, because it is answered on the mouse-move path.
    var brushRingVisible: Bool {
        guard let ptr else { return false }
        return calm_engine_brush_ring_visible(ptr) != 0
    }

    func toggleTransform() {
        guard let ptr else { return }
        _ = calm_engine_toggle_transform(ptr)
        render()
        syncState()
    }

    /// Idempotent, unlike `toggleTransform` — pasting always wants to *be* in transform, not
    /// to flip whatever the board was already doing.
    func enterTransform() {
        guard let ptr else { return }
        _ = calm_engine_enter_transform(ptr)
        render()
        syncState()
    }

    func exitTransform() {
        guard let ptr else { return }
        _ = calm_engine_exit_transform(ptr)
        render()
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

    func moveLayerRow(from: Int, to: Int) {
        guard let ptr, from != to else { return }
        _ = calm_engine_move_layer_row(ptr, UInt32(from), UInt32(to))
        syncState()
        refreshLayers()
        render()
    }

    @discardableResult
    func setLayerName(_ index: Int, name: String) -> Bool {
        guard let ptr else { return false }
        let ok = name.withCString { calm_engine_set_layer_name(ptr, UInt32(index), $0) } == CalmStatusOk
        if ok { refreshLayers() }
        return ok
    }

    func setLayerLocked(_ index: Int, locked: Bool) {
        guard let ptr else { return }
        _ = calm_engine_set_layer_locked(ptr, UInt32(index), locked ? 1 : 0)
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

    /// Bakes the layer through the alpha of the one below it and merges the two. Destructive the
    /// moment it is pressed — there is no clipped state afterwards, which is the whole reason the
    /// renderer never has to know the word.
    func clipLayerDown(_ index: Int) {
        guard let ptr else { return }
        _ = calm_engine_clip_layer_down(ptr, UInt32(index))
        syncState()
        refreshLayers()
        render()
    }

    /// Merge Down's rules plus a raster base carrying no transform, all answered by the engine so
    /// the greyed-out button and the refused call can never disagree.
    func canClipLayerDown(index: Int) -> Bool {
        guard let ptr, index >= 0 else { return false }
        return calm_engine_layer_can_clip_down(ptr, UInt32(index)) != 0
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
        layerLocked = []
        layerOpacities = []
        layerBlendModes = []
        layerAdjustments = []
        layerIsText = []
        textEditing = false
    }

    func project(id: String) -> ProjectInfo? {
        guard let ptr else { return nil }
        var info = CalmProjectInfo(
            id: nil,
            name: nil,
            width: 0,
            height: 0,
            opened_at: 0,
            accent: 0
        )
        let status = id.withCString { calm_project_get(ptr, $0, &info) }
        guard status == CalmStatusOk, let idPtr = info.id, let namePtr = info.name else {
            return nil
        }
        let item = ProjectInfo(
            id: String(cString: idPtr),
            name: String(cString: namePtr),
            width: Int(info.width),
            height: Int(info.height),
            openedAt: info.opened_at,
            accent: info.accent
        )
        calm_string_free(idPtr)
        calm_string_free(namePtr)
        return item
    }

    func loadOpenProjectTabs() -> [String] {
        guard let ptr else { return [] }
        var buffer = Array(repeating: Optional<UnsafeMutablePointer<CChar>>.none, count: 64)
        let count = buffer.withUnsafeMutableBufferPointer {
            calm_open_project_tabs(ptr, $0.baseAddress, 64)
        }
        var ids: [String] = []
        for i in 0..<count {
            guard let idPtr = buffer[i] else { continue }
            ids.append(String(cString: idPtr))
            calm_string_free(idPtr)
        }
        return ids
    }

    func persistOpenProjectTabs(_ ids: [String]) {
        guard let ptr else { return }
        if ids.isEmpty {
            _ = calm_set_open_project_tabs(ptr, nil, 0)
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
                _ = calm_set_open_project_tabs(ptr, rebound, ids.count)
            }
        }
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
            + out.text_bytes + out.preview_bytes + out.gpu_bytes
    }

    func bumpThumbnailRevision() {
        thumbnailRevision &+= 1
    }

    /// Bumped by `EngineGuides.syncGuides` after any change to the list — the setter stays here
    /// with the property, the way `thumbnailRevision` does.
    func bumpGuidesRevision() {
        guidesRevision &+= 1
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
            lastSelectTool: CalmTool(rawValue: raw.last_select_tool) ?? .selectRect,
            isFit: raw.is_fit != 0,
            transformActive: raw.transform_active != 0
        )
        syncGuideCount()
        syncToolGate()
    }

    /// `guideCount` is `private(set)`, so this is the one place it moves — the guide bridge in
    /// `EngineGuides.swift` calls it after anything that could add or drop a rule.
    func syncGuideCount() {
        guard let ptr else { return }
        let count = calm_engine_guide_count(ptr)
        if guideCount != count {
            guideCount = count
        }
    }

    func refreshLayers() {
        guard let ptr else { return }
        var ids: [String] = []
        var revisions: [UInt64] = []
        var names: [String] = []
        var visibles: [Bool] = []
        var opacities: [Float] = []
        var blendModes: [CalmBlendMode] = []
        var adjustments: [LayerAdjustments] = []
        var isText: [Bool] = []
        var locked: [Bool] = []
        for i in 0..<state.layerCount {
            if let namePtr = calm_engine_layer_name(ptr, i) {
                let raw = String(cString: namePtr)
                calm_string_free(namePtr)
                names.append(raw == "Paper" ? L10nStore.catalog.paper : raw)
            } else {
                names.append(L10nStore.catalog.formatKey("layerNamed", "\(i + 1)"))
            }
            if let idPtr = calm_engine_layer_id(ptr, i) {
                ids.append(String(cString: idPtr))
                calm_string_free(idPtr)
            } else {
                ids.append("layer-\(i)")
            }
            revisions.append(calm_engine_layer_preview_revision(ptr, i))
            visibles.append(calm_engine_layer_visible(ptr, i) == 1)
            isText.append(calm_engine_layer_is_text(ptr, i) == 1)
            locked.append(calm_engine_layer_locked(ptr, i) == 1)
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
        layerLocked = locked
        rebuildLayerThumbnails(ids: ids, revisions: revisions)
        syncTextState()
    }

    /// The active layer's document-space box, or `nil` for a layer with nothing to bound.
    func layerBounds(index: Int) -> CalmLayerBounds? {
        guard let ptr, index >= 0 else { return nil }
        var out = CalmLayerBounds()
        guard calm_engine_layer_bounds(ptr, UInt32(index), &out) == CalmStatusOk else { return nil }
        return out
    }

    /// Moves the layer and crops it to the given box. The engine clamps a size larger than the
    /// layer already is rather than scaling it up, so callers read the bounds back afterwards
    /// instead of assuming what they asked for is what landed.
    func setLayerBounds(index: Int, x: Float, y: Float, width: Float, height: Float) {
        guard let ptr, index >= 0 else { return }
        _ = calm_engine_set_layer_bounds(ptr, UInt32(index), x, y, width, height)
        syncState()
        refreshLayers()
        render()
    }

    func layerThumbnail(index: Int) -> NSImage? {
        layerThumbnails.indices.contains(index) ? layerThumbnails[index] : nil
    }

    func layerPreviewCard(index: Int) -> NSImage? {
        layerPreviewCards.indices.contains(index) ? layerPreviewCards[index] : nil
    }

    /// Rebuilds the parallel thumbnail array, reusing every image whose layer has not been
    /// painted on since it was made. `ids` and `revisions` come from the same pass that reads
    /// the rest of the layer state, so this costs one dictionary probe per layer in the common
    /// case and touches the engine only for layers that actually changed.
    private func rebuildLayerThumbnails(ids: [String], revisions: [UInt64]) {
        var rows: [NSImage?] = []
        var cards: [NSImage?] = []
        var memo: [String: LayerThumbnailEntry] = [:]
        rows.reserveCapacity(ids.count)
        cards.reserveCapacity(ids.count)
        for (index, id) in ids.enumerated() {
            let revision = revisions[index]
            if let hit = layerThumbnailMemo[id], hit.revision == revision {
                memo[id] = hit
                rows.append(hit.row)
                cards.append(hit.card)
                continue
            }
            let card = renderLayerThumbnail(index: index, maxSide: Engine.layerPreviewSide)
            let entry = LayerThumbnailEntry(revision: revision, row: rowThumbnail(from: card), card: card)
            memo[id] = entry
            rows.append(entry.row)
            cards.append(entry.card)
        }
        layerThumbnailMemo = memo
        layerThumbnails = rows
        layerPreviewCards = cards
    }

    /// Redraws a preview once at exactly the size the layer row shows it, cropped to fill.
    ///
    /// The row used to take the full-size preview and lean on `.resizable()` +
    /// `.aspectRatio(.fill)` to fit it — which made AppKit resample an interpolated image down
    /// to 40×28 on every pass of every row's body. Baking it here means the view draws 1:1 and
    /// the scaling happens once per layer edit rather than once per re-render.
    private func rowThumbnail(from source: NSImage?) -> NSImage? {
        guard let source,
              let cg = source.cgImage(forProposedRect: nil, context: nil, hints: nil)
        else { return nil }
        let w = Engine.rowThumbPixelSize.width
        let h = Engine.rowThumbPixelSize.height
        guard let ctx = CGContext(
            data: nil,
            width: Int(w),
            height: Int(h),
            bitsPerComponent: 8,
            bytesPerRow: Int(w) * 4,
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        ) else { return nil }
        ctx.interpolationQuality = .high
        let sw = CGFloat(cg.width)
        let sh = CGFloat(cg.height)
        let scale = max(w / sw, h / sh)
        let dw = sw * scale
        let dh = sh * scale
        ctx.draw(cg, in: CGRect(x: (w - dw) / 2, y: (h - dh) / 2, width: dw, height: dh))
        guard let out = ctx.makeImage() else { return nil }
        return NSImage(cgImage: out, size: Engine.rowThumbPointSize)
    }

    private static let layerPreviewSide: UInt32 = 160
    /// Backing pixels for a row thumb at 2×, and the point size it is drawn at.
    private static let rowThumbPixelSize = CGSize(width: 80, height: 56)
    private static let rowThumbPointSize = NSSize(width: 40, height: 28)

    private func renderLayerThumbnail(index: Int, maxSide: UInt32) -> NSImage? {
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

    /// The document as one PDF. Layered like the SVG export: vector layers stay real PDF
    /// paths and layer opacity/blend ride graphics state, so it is built from engine bytes
    /// rather than from a composited image.
    func exportPDF(dpi: Float = calm_pdf_default_dpi()) -> Data? {
        guard let ptr else { return nil }
        var bytesPtr: UnsafeMutablePointer<UInt8>?
        var len: Int = 0
        let status = calm_engine_export_pdf(ptr, dpi, &bytesPtr, &len)
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

    /// An ordinary layer of pixels — the engine's `Layer::is_raster()`, which is deliberately
    /// **false** for a text layer as well as a vector one: text tiles are a cache of the run
    /// and the run is the source of truth.
    func isLayerRaster(index: Int) -> Bool {
        !isLayerVector(index: index) && !isLayerText(index: index)
    }

    func isLayerPaper(index: Int) -> Bool {
        guard let ptr else { return false }
        return calm_engine_layer_is_paper(ptr, UInt32(index)) == 1
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

    func selectAll() {
        guard let ptr else { return }
        _ = calm_engine_select_all(ptr)
        render()
    }

    func invertSelection() {
        guard let ptr else { return }
        _ = calm_engine_invert_selection(ptr)
        render()
    }

    func clearSelectionPixels() {
        guard let ptr else { return }
        _ = calm_engine_selection_clear_pixels(ptr)
        syncState()
        render()
    }

    /// Pastes at native size and reports whether it fit on the paper, so the caller can say so.
    @discardableResult
    func pasteImage(premultipliedRGBA: Data, width: Int, height: Int) -> CalmPasteOutcome {
        guard let ptr else { return .failed }
        var raw: UInt32 = 0
        premultipliedRGBA.withUnsafeBytes { bytes in
            guard let base = bytes.bindMemory(to: UInt8.self).baseAddress else { return }
            _ = calm_engine_paste_image(
                ptr, base, premultipliedRGBA.count, UInt32(width), UInt32(height), &raw
            )
        }
        syncState()
        refreshLayers()
        render()
        return CalmPasteOutcome(rawValue: raw) ?? .failed
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
        guard isLayerRaster(index: layerIndex) else {
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

/// `calumma_core::DeskMetrics` on the Swift side. Screen points, so the pattern holds still
/// while the paper pans and zooms over it.
struct DeskMetrics {
    var cell: CGFloat
    var lineWidth: CGFloat
    var crossArm: CGFloat
    var crossLineWidth: CGFloat
    var lineAlpha: Double

    init(_ raw: CalmDeskMetrics) {
        cell = CGFloat(raw.cell)
        lineWidth = CGFloat(raw.line_width)
        crossArm = CGFloat(raw.cross_arm)
        crossLineWidth = CGFloat(raw.cross_line_width)
        lineAlpha = Double(raw.line_alpha)
    }
}
