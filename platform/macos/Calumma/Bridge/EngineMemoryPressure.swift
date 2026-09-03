import Dispatch
import Foundation

/// The raw values `calm_engine_set_memory_pressure` reads, mirrored from
/// `calumma_core::MemoryPressureLevel` (`docs/plans/22-adaptive-gpu-memory-pressure.md`). Crossed
/// as a plain `UInt32` rather than a declared C enum type — the same convention `CalmTool` uses.
enum CalmMemoryPressure: UInt32 {
    case normal = 0
    case warn = 1
    case critical = 2

    /// `DispatchSource.MemoryPressureEvent` is a bitmask (`.normal`/`.warning`/`.critical`) that
    /// can in principle carry more than one bit; the source is masked to exactly these three, so
    /// this only has to pick the worst one set.
    init(_ event: DispatchSource.MemoryPressureEvent) {
        if event.contains(.critical) {
            self = .critical
        } else if event.contains(.warning) {
            self = .warn
        } else {
            self = .normal
        }
    }
}

extension Engine {
    /// There is no `didReceiveMemoryWarning` on macOS — that is iOS — and
    /// `os_proc_available_memory()` is iOS-only too, so a dispatch memory-pressure source is the
    /// one correct entry point for what the OS itself thinks of the machine's memory. Started
    /// once per `Engine` and cancelled in `deinit`; the level it reports is forwarded to the
    /// renderer and nothing else — the shell makes no residency decisions of its own.
    func startObservingMemoryPressure() {
        let source = DispatchSource.makeMemoryPressureSource(
            eventMask: [.normal, .warning, .critical],
            queue: .main
        )
        source.setEventHandler { [weak self] in
            guard let self, let ptr = self.ptr else { return }
            let level = CalmMemoryPressure(source.data)
            _ = calm_engine_set_memory_pressure(ptr, level.rawValue)
        }
        source.resume()
        memoryPressureSource = source
    }
}
