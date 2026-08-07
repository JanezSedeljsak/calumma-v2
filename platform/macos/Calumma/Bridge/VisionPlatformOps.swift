import CoreGraphics
import Foundation
import Vision

enum VisionPlatformOps {
    static func install(into engine: OpaquePointer) {
        var ops = CalmPlatformOps(
            available: available,
            run: run,
            free_output: freeOutput
        )
        _ = calm_engine_install_platform_ops(engine, &ops)
    }

    private static let available: CalmOpAvailableFn = { kind in
        kind == CalmOpKindRemoveBackground
    }

    private static let run: CalmOpRunFn = { kind, input, out in
        guard kind == CalmOpKindRemoveBackground else { return CalmStatusError }
        guard let input, let out else { return CalmStatusNull }
        guard let rgba = input.pointee.rgba else { return CalmStatusError }
        let w = Int(input.pointee.w)
        let h = Int(input.pointee.h)
        guard w > 0, h > 0 else { return CalmStatusError }

        let byteCount = w * h * 4
        let data = Data(bytes: rgba, count: byteCount)
        guard let provider = CGDataProvider(data: data as CFData) else { return CalmStatusError }
        guard let image = CGImage(
            width: w,
            height: h,
            bitsPerComponent: 8,
            bitsPerPixel: 32,
            bytesPerRow: w * 4,
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.last.rawValue),
            provider: provider,
            decode: nil,
            shouldInterpolate: false,
            intent: .defaultIntent
        ) else {
            return CalmStatusError
        }

        let request = VNGenerateForegroundInstanceMaskRequest()
        let handler = VNImageRequestHandler(cgImage: image, options: [:])
        do {
            try handler.perform([request])
            guard let observation = request.results?.first else { return CalmStatusError }
            let maskBuffer = try observation.generateScaledMaskForImage(
                forInstances: observation.allInstances,
                from: handler
            )
            guard let mask = maskBytes(from: maskBuffer, width: w, height: h) else {
                return CalmStatusError
            }
            let raw = UnsafeMutablePointer<UInt8>.allocate(capacity: mask.count)
            mask.copyBytes(to: raw, count: mask.count)
            out.pointee.kind = CalmOpOutputKindMask
            out.pointee.data = raw
            out.pointee.len = mask.count
            out.pointee.w = UInt32(w)
            out.pointee.h = UInt32(h)
            return CalmStatusOk
        } catch {
            return CalmStatusError
        }
    }

    private static let freeOutput: CalmOpFreeFn = { out in
        guard let out else { return }
        if let data = out.pointee.data {
            data.deallocate()
            out.pointee.data = nil
        }
        out.pointee.len = 0
        out.pointee.kind = CalmOpOutputKindNone
        out.pointee.w = 0
        out.pointee.h = 0
    }

    private static func maskBytes(from buffer: CVPixelBuffer, width: Int, height: Int) -> Data? {
        CVPixelBufferLockBaseAddress(buffer, .readOnly)
        defer { CVPixelBufferUnlockBaseAddress(buffer, .readOnly) }
        guard let base = CVPixelBufferGetBaseAddress(buffer) else { return nil }
        let srcW = CVPixelBufferGetWidth(buffer)
        let srcH = CVPixelBufferGetHeight(buffer)
        let stride = CVPixelBufferGetBytesPerRow(buffer)
        let format = CVPixelBufferGetPixelFormatType(buffer)

        var out = Data(count: width * height)
        out.withUnsafeMutableBytes { dest in
            guard let dst = dest.bindMemory(to: UInt8.self).baseAddress else { return }
            for y in 0..<height {
                let sy = min(srcH - 1, y * srcH / max(height, 1))
                for x in 0..<width {
                    let sx = min(srcW - 1, x * srcW / max(width, 1))
                    let value: UInt8
                    if format == kCVPixelFormatType_OneComponent8 {
                        let row = base.advanced(by: sy * stride).assumingMemoryBound(to: UInt8.self)
                        value = row[sx]
                    } else {
                        let row = base.advanced(by: sy * stride).assumingMemoryBound(to: Float32.self)
                        let f = max(0, min(1, row[sx]))
                        value = UInt8((f * 255).rounded())
                    }
                    dst[y * width + x] = value
                }
            }
        }
        return out
    }
}
