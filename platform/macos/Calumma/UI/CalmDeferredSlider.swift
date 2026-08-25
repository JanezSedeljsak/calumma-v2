import SwiftUI

/// A slider whose knob answers to the pointer and whose engine call answers to a timer.
///
/// A filter slider used to hand every intermediate value it produced straight to
/// `calm_engine_set_layer_adjustments`, and each of those rebuilds the layer's adjustment LUT
/// and re-bakes its tiles on the CPU before the next upload — so one drag across the track
/// bought a full-layer bake per value the track emitted. The knob here is local state and
/// moves at pointer speed; the engine only ever hears the value still standing once the
/// pointer has been quiet for `settle`, which collapses a whole drag into a handful of bakes
/// while staying far under the ~200ms where a delay stops reading as "the app is working" and
/// starts reading as "the app is stuck".
///
/// The draft is dropped when the engine is told, not when it answers, so a value the engine
/// clamps lands the knob on the clamped value rather than on the one the pointer asked for.
struct CalmDeferredSlider: View {
    let value: Float
    let range: ClosedRange<Float>
    var settle: Duration = .milliseconds(100)
    let onSettled: (Float) -> Void

    @State private var draft: Float?
    @State private var settleTask: Task<Void, Never>?

    var body: some View {
        Slider(
            value: Binding(
                get: { Double(draft ?? value) },
                set: { schedule(Float($0)) }
            ),
            in: Double(range.lowerBound)...Double(range.upperBound)
        )
        .controlSize(.mini)
    }

    private func schedule(_ next: Float) {
        draft = next
        settleTask?.cancel()
        settleTask = Task { @MainActor in
            try? await Task.sleep(for: settle)
            guard !Task.isCancelled else { return }
            settleTask = nil
            onSettled(next)
            draft = nil
        }
    }
}
