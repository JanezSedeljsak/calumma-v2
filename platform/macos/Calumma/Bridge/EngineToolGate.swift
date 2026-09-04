import Foundation

/// Why a tool cannot run on the active layer. The engine owns every rule behind this; the
/// shell only greys a button out and repeats the reason.
enum CalmToolBlock: UInt32 {
    case none = 0
    case layerLocked = 1
    case textLayer = 2
    case vectorLayer = 3
    case noContent = 4

    var blocks: Bool { self != .none }

    /// The reason, in the user's language. `none` has nothing to say, so callers fall back to
    /// the tool's own name.
    func reason(_ l10n: L10nCatalog) -> String? {
        switch self {
        case .none: return nil
        case .layerLocked: return l10n.toolBlockedLocked
        case .textLayer: return l10n.toolBlockedText
        case .vectorLayer: return l10n.toolBlockedVector
        case .noContent: return l10n.toolBlockedEmpty
        }
    }
}

extension Engine {
    /// One slot per `Tool` discriminant in the engine, so the table can be read whole and then
    /// indexed by `CalmTool.rawValue` without a second call per button. One more than the
    /// highest discriminant (`Tool::Heal = 20`).
    static let toolSlots = 21

    func toolBlock(_ tool: CalmTool) -> CalmToolBlock {
        let index = Int(tool.rawValue)
        return toolBlocks.indices.contains(index) ? toolBlocks[index] : .none
    }

    func isBlocked(_ tool: CalmTool) -> Bool {
        toolBlock(tool).blocks
    }

    /// Reads the whole rule table, plus the two things that follow from it: whether the active
    /// layer pins vector mode on, and whether a press was just refused. Called from
    /// `syncState`, which is every point at which the active layer, its flags or the mode can
    /// have changed — not per frame.
    func syncToolGate() {
        guard let ptr else { return }
        var raw = [UInt32](repeating: 0, count: Self.toolSlots)
        let written = raw.withUnsafeMutableBufferPointer { buffer in
            Int(calm_engine_tool_blocks(ptr, buffer.baseAddress, UInt32(buffer.count)))
        }
        let next = raw.prefix(written).map { CalmToolBlock(rawValue: $0) ?? .none }
        if toolBlocks != Array(next) {
            toolBlocks = Array(next)
        }
        let locked = calm_engine_vector_mode_locked(ptr) != 0
        if vectorModeLocked != locked {
            vectorModeLocked = locked
        }
        var notice: UInt32 = 0
        if calm_engine_take_tool_block_notice(ptr, &notice) == CalmStatusOk,
           let block = CalmToolBlock(rawValue: notice), block.blocks
        {
            toolBlockNotice = block
        }
    }

    /// Cleared by whoever showed it, so the next refusal of the same kind is a change again and
    /// reaches the toast rather than landing on an identical value.
    func clearToolBlockNotice() {
        guard toolBlockNotice != .none else { return }
        toolBlockNotice = .none
    }

    func isLayerRasterizable(index: Int) -> Bool {
        guard let ptr, index >= 0 else { return false }
        return calm_engine_layer_is_rasterizable(ptr, UInt32(index)) != 0
    }

    /// Turns a live layer — text or vector — into ordinary pixels, which is the way out of
    /// every block those two impose. One way: the run or the item is gone afterwards, which is
    /// why it is an explicit command and never a side effect of picking up a brush.
    func rasterizeLayer(_ index: Int) {
        guard let ptr else { return }
        _ = calm_engine_rasterize_layer(ptr, UInt32(index))
        syncState()
        refreshLayers()
        render()
    }
}
