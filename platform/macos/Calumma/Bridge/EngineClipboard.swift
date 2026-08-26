import AppKit
import Foundation

enum CalmClipboardKind: UInt32 {
    case png = 0
    case svg = 1
}

extension Engine {
    func copy() -> (Data, CalmClipboardKind)? {
        clipboardPayload(cut: false)
    }

    func cut() -> (Data, CalmClipboardKind)? {
        clipboardPayload(cut: true)
    }

    func copyLayer(index: Int) -> (Data, CalmClipboardKind)? {
        guard let ptr else { return nil }
        var bytesPtr: UnsafeMutablePointer<UInt8>?
        var len = 0
        var kind: UInt32 = 0
        let status = calm_engine_copy_layer(ptr, UInt32(index), &bytesPtr, &len, &kind)
        return Self.takeClipboard(status: status, bytesPtr: bytesPtr, len: len, kind: kind)
    }

    func nudgeMoveTarget(x: Float, y: Float) -> Bool {
        guard let ptr else { return false }
        return calm_engine_nudge_move_target(ptr, x, y) != 0
    }

    private func clipboardPayload(cut: Bool) -> (Data, CalmClipboardKind)? {
        guard let ptr else { return nil }
        var bytesPtr: UnsafeMutablePointer<UInt8>?
        var len = 0
        var kind: UInt32 = 0
        let status = cut
            ? calm_engine_cut(ptr, &bytesPtr, &len, &kind)
            : calm_engine_copy(ptr, &bytesPtr, &len, &kind)
        let payload = Self.takeClipboard(status: status, bytesPtr: bytesPtr, len: len, kind: kind)
        if cut, payload != nil {
            syncState()
        }
        return payload
    }

    private static func takeClipboard(
        status: CalmStatus,
        bytesPtr: UnsafeMutablePointer<UInt8>?,
        len: Int,
        kind: UInt32
    ) -> (Data, CalmClipboardKind)? {
        guard status == CalmStatusOk, let bytesPtr, len > 0 else { return nil }
        let data = Data(bytes: bytesPtr, count: len)
        calm_buffer_free(bytesPtr, len)
        return (data, CalmClipboardKind(rawValue: kind) ?? .png)
    }
}
