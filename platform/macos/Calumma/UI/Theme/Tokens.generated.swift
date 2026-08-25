import SwiftUI

enum Tokens {
    enum Radius {
        static let sm: CGFloat = 4.0
        static let md: CGFloat = 6.0
        static let lg: CGFloat = 10.0
        static let window: CGFloat = 12.0
        static let island: CGFloat = 10.0
    }

    enum Space {
        static let xs: CGFloat = 4.0
        static let sm: CGFloat = 8.0
        static let md: CGFloat = 12.0
        static let lg: CGFloat = 16.0
        static let xl: CGFloat = 24.0
        static let xxl: CGFloat = 32.0
    }

    enum Control {
        static let height: CGFloat = 28.0
    }

    enum TypeSize {
        static let label: CGFloat = 10.0
        static let labelTracking: CGFloat = 0.08
        static let body: CGFloat = 13.0
        static let title: CGFloat = 15.0
        static let brand: CGFloat = 36.0
    }

    enum Window {
        static let mainWidth: CGFloat = 960.0
        static let mainHeight: CGFloat = 520.0
        static let mainMinWidth: CGFloat = 760.0
        static let mainMinHeight: CGFloat = 520.0
        static let newProjectWidth: CGFloat = 960.0
        static let newProjectHeight: CGFloat = 520.0
        static let newProjectMinWidth: CGFloat = 960.0
        static let newProjectMinHeight: CGFloat = 520.0
        static let pasteMinWidth: CGFloat = 200.0
        static let pasteMaxWidth: CGFloat = 420.0
        static let pasteMinHeight: CGFloat = 180.0
        static let pasteWidthRatio: CGFloat = 0.32
    }

    enum Light {
    static let bg = Color(red: 0.909804, green: 0.933333, blue: 0.949020, opacity: 1.000000)
    static let surface = Color(red: 0.956863, green: 0.968627, blue: 0.976471, opacity: 1.000000)
    static let surfaceHover = Color(red: 0.886275, green: 0.909804, blue: 0.933333, opacity: 1.000000)
    static let text = Color(red: 0.070588, green: 0.094118, blue: 0.109804, opacity: 1.000000)
    static let textMuted = Color(red: 0.360784, green: 0.419608, blue: 0.458824, opacity: 1.000000)
    static let danger = Color(red: 0.839216, green: 0.270588, blue: 0.270588, opacity: 1.000000)
    static let desk = Color(red: 0.956863, green: 0.968627, blue: 0.976471, opacity: 1.000000)
    static let deskGrid = Color(red: 0.717647, green: 0.768627, blue: 0.807843, opacity: 1.000000)
    static let paper = Color(red: 1.000000, green: 1.000000, blue: 1.000000, opacity: 1.000000)
    static let paperBorder = Color(red: 0.058824, green: 0.105882, blue: 0.141176, opacity: 0.349020)
    static let islandBorder = Color(red: 0.058824, green: 0.105882, blue: 0.141176, opacity: 0.121569)
    static let controlBorder = Color(red: 0.058824, green: 0.105882, blue: 0.141176, opacity: 0.250980)
    static let controlFocusBorder = Color(red: 0.168627, green: 0.721569, blue: 0.784314, opacity: 0.800000)
    static let accentTeal = Color(red: 0.168627, green: 0.721569, blue: 0.784314, opacity: 1.000000)
    static let accentOrange = Color(red: 0.909804, green: 0.529412, blue: 0.227451, opacity: 1.000000)
    }

    enum Dark {
    static let bg = Color(red: 0.086275, green: 0.109804, blue: 0.125490, opacity: 1.000000)
    static let surface = Color(red: 0.101961, green: 0.125490, blue: 0.141176, opacity: 1.000000)
    static let surfaceHover = Color(red: 0.141176, green: 0.172549, blue: 0.196078, opacity: 1.000000)
    static let text = Color(red: 0.949020, green: 0.960784, blue: 0.968627, opacity: 1.000000)
    static let textMuted = Color(red: 0.541176, green: 0.592157, blue: 0.627451, opacity: 1.000000)
    static let danger = Color(red: 0.909804, green: 0.352941, blue: 0.352941, opacity: 1.000000)
    static let desk = Color(red: 0.039216, green: 0.047059, blue: 0.058824, opacity: 1.000000)
    static let deskGrid = Color(red: 0.070588, green: 0.094118, blue: 0.125490, opacity: 1.000000)
    static let paper = Color(red: 0.109804, green: 0.141176, blue: 0.168627, opacity: 1.000000)
    static let paperBorder = Color(red: 1.000000, green: 1.000000, blue: 1.000000, opacity: 0.349020)
    static let islandBorder = Color(red: 1.000000, green: 1.000000, blue: 1.000000, opacity: 0.121569)
    static let controlBorder = Color(red: 1.000000, green: 1.000000, blue: 1.000000, opacity: 0.200000)
    static let controlFocusBorder = Color(red: 0.235294, green: 0.788235, blue: 0.839216, opacity: 0.800000)
    static let accentTeal = Color(red: 0.235294, green: 0.788235, blue: 0.839216, opacity: 1.000000)
    static let accentOrange = Color(red: 0.941176, green: 0.580392, blue: 0.290196, opacity: 1.000000)
    }

    struct Preset: Identifiable, Hashable {
        let id: String
        let label: String
        let width: Int
        let height: Int
    }

    static let presets: [Preset] = [
        Preset(id: "169-hd", label: "16:9 HD", width: 1920, height: 1080),
        Preset(id: "square", label: "Square", width: 1080, height: 1080),
        Preset(id: "a4-portrait", label: "A4 Portrait", width: 2480, height: 3508),
        Preset(id: "a4-landscape", label: "A4 Landscape", width: 3508, height: 2480),
        Preset(id: "4k-cinema", label: "4K Cinema", width: 3840, height: 2160),
    ]
}
