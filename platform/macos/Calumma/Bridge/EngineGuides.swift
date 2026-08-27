import CoreGraphics
import Foundation
import SwiftUI

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
        syncGuides()
        refreshGuideReadout()
    }

    func updateGuideDrag(x: Float, y: Float) {
        guard let ptr else { return }
        _ = calm_engine_guide_drag_update(ptr, x, y)
        refreshGuideReadout()
    }

    func endGuideDrag(x: Float, y: Float) {
        guard let ptr else { return }
        _ = calm_engine_guide_drag_end(ptr, x, y)
        syncGuides()
        refreshGuideReadout()
    }

    /// Where the guide in flight currently sits, or `nil` when none is. Deliberately *not* part
    /// of `syncState`: a drag updates on every pointer move, and the rest of that struct has no
    /// reason to be re-read at that rate — so this is one small call the drag paths make for
    /// themselves, onto an observable of its own (`GuideReadoutStore`).
    func refreshGuideReadout() {
        guard let ptr else { return }
        var axis: UInt8 = 0
        var position: Float = 0
        var screen: Float = 0
        guard calm_engine_dragged_guide(ptr, &axis, &position, &screen) != 0,
              let axis = CalmGuideAxis(rawValue: axis)
        else {
            guideReadout.set(nil)
            return
        }
        guideReadout.set(GuideReadout(axis: axis, position: position, screen: CGFloat(screen)))
    }

    func clearGuides() {
        guard let ptr else { return }
        _ = calm_engine_clear_guides(ptr)
        syncGuides()
    }

    /// The whole list, for the guides card. Read on demand rather than mirrored in a
    /// `@Published` array: the card is the only thing that wants it, and it is open for the few
    /// seconds someone is editing guides.
    func guideList() -> [GuideEntry] {
        guard let ptr else { return [] }
        var buffer = [CalmGuide](repeating: CalmGuide(axis: 0, position: 0), count: Self.guidesCap)
        let count = buffer.withUnsafeMutableBufferPointer {
            calm_engine_guide_list(ptr, $0.baseAddress, Self.guidesCap)
        }
        return buffer.prefix(count).enumerated().compactMap { index, raw in
            guard let axis = CalmGuideAxis(rawValue: raw.axis) else { return nil }
            return GuideEntry(index: index, axis: axis, position: raw.position)
        }
    }

    /// `limits::GUIDES_LIMIT`, asked rather than written down: the card greys its Add out on it,
    /// and a stale copy would either hide a row or offer a guide the engine then refuses.
    static let guidesCap = calm_guides_limit()

    func addGuide(axis: CalmGuideAxis, position: Float) {
        guard let ptr else { return }
        _ = calm_engine_add_guide(ptr, axis.rawValue, position)
        syncGuides()
    }

    func setGuidePosition(index: Int, position: Float) {
        guard let ptr else { return }
        _ = calm_engine_set_guide_position(ptr, index, position)
        syncGuides()
    }

    func setGuideAxis(index: Int, axis: CalmGuideAxis) {
        guard let ptr else { return }
        _ = calm_engine_set_guide_axis(ptr, index, axis.rawValue)
        syncGuides()
    }

    func removeGuide(index: Int) {
        guard let ptr else { return }
        _ = calm_engine_remove_guide(ptr, index)
        syncGuides()
    }

    /// Guides changed: republish the count the Board menu reads, bump the revision the card
    /// reloads on, and ask for a frame — none of these go through a pointer path that would have
    /// invalidated for us.
    ///
    /// The revision exists because the count is not enough. Flipping a guide's axis or moving it
    /// leaves the list exactly as long as it was, so a card watching only the count kept showing
    /// what the guide used to be while the board drew what it had become.
    private func syncGuides() {
        syncGuideCount()
        bumpGuidesRevision()
        render()
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

/// A guide mid-drag, as the board's readout draws it: which way the rule runs, the document
/// pixel it is on, and where that lands on screen along its own axis. Every number is the
/// engine's (`Document::dragged_guide_readout`) — the shell only formats and positions them.
struct GuideReadout: Equatable {
    var axis: CalmGuideAxis
    var position: Float
    var screen: CGFloat

    var label: String { "\(Int(position.rounded()))" }
}

/// The one thing a guide drag publishes. Kept off `Engine` so a drag does not republish the
/// engine on every pointer move — that would re-render every view watching `AppModel`, which is
/// the cost `Engine.pointerMove` goes out of its way not to pay during a stroke. Only the
/// readout label observes this, so only the readout label redraws.
final class GuideReadoutStore: ObservableObject {
    @Published private(set) var readout: GuideReadout?

    /// Assigns only on a change: a move that does not shift the guide — clamped, or snapped to
    /// where it already was — publishes nothing.
    func set(_ next: GuideReadout?) {
        if readout != next { readout = next }
    }
}

/// One row of the guides card: a guide, and the index the engine knows it by. The index is only
/// valid until the list changes, which is why every edit re-reads it.
struct GuideEntry: Identifiable, Equatable {
    let index: Int
    var axis: CalmGuideAxis
    var position: Float

    var id: Int { index }
}
