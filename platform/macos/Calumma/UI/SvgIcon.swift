import AppKit
import SwiftUI

enum SvgIconStore {
    private static var cache: [String: NSImage] = [:]

    static func image(named name: String) -> NSImage {
        if let cached = cache[name] {
            return cached
        }
        let image = load(name: name) ?? NSImage(size: NSSize(width: 18, height: 18))
        image.isTemplate = true
        cache[name] = image
        return image
    }

    private static func load(name: String) -> NSImage? {
        if let url = Bundle.main.url(forResource: name, withExtension: "svg", subdirectory: "icons")
            ?? Bundle.main.url(forResource: name, withExtension: "svg")
        {
            return NSImage(contentsOf: url)
        }
        let root = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
        let url = root
            .appendingPathComponent("design/icons", isDirectory: true)
            .appendingPathComponent("\(name).svg")
        return FileManager.default.fileExists(atPath: url.path) ? NSImage(contentsOf: url) : nil
    }
}

struct SvgIcon: View {
    let name: String
    let color: Color
    var size: CGFloat = 18

    var body: some View {
        Image(nsImage: SvgIconStore.image(named: name))
            .resizable()
            .renderingMode(.template)
            .frame(width: size, height: size)
            .foregroundStyle(color)
            .accessibilityHidden(true)
    }
}
