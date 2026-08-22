import Foundation

/// Which way a guide runs, mirroring `calumma_core::GuideAxis`. A horizontal guide is the one
/// pulled off the top ruler; a vertical guide comes off the left one.
enum CalmGuideAxis: UInt8 {
    case horizontal = 0
    case vertical = 1
}

/// Guides dragged *on the board* need nothing here — they ride `pointerDown/Move/Up` with the
/// Move tool, decided engine-side. These entry points exist for the rulers, which are SwiftUI
/// views: once a drag starts on a ruler strip the pointer events belong to that view, so it has
/// to drive the same engine gesture itself. Every coordinate below is a **board** screen point,
/// which is why a drag still inside the ruler is negative — and why releasing it there discards
/// the guide without the shell having to know that rule.
extension Engine {
    func beginGuideDragFromRuler(axis: CalmGuideAxis, x: Float, y: Float) {
        guard let ptr else { return }
        _ = calm_engine_guide_drag_from_ruler(ptr, axis.rawValue, x, y)
        syncGuideCount()
    }

    func updateGuideDrag(x: Float, y: Float) {
        guard let ptr else { return }
        _ = calm_engine_guide_drag_update(ptr, x, y)
    }

    func endGuideDrag(x: Float, y: Float) {
        guard let ptr else { return }
        _ = calm_engine_guide_drag_end(ptr, x, y)
        syncGuideCount()
    }

    func clearGuides() {
        guard let ptr else { return }
        _ = calm_engine_clear_guides(ptr)
        syncGuideCount()
    }

    /// The axis of the guide under a board point, or `nil` — all the board needs to offer a
    /// grab cursor before the click lands.
    func guideAxis(atX x: Float, y: Float) -> CalmGuideAxis? {
        guard let ptr else { return nil }
        let raw = calm_engine_guide_axis_at(ptr, x, y)
        guard raw >= 0 else { return nil }
        return CalmGuideAxis(rawValue: UInt8(raw))
    }
}
