import SwiftUI

/// Which glyph stands for a tool or a brush. Shared because the tool grid, the tool options
/// below it and the **board cursor** all draw the same families: the grid shows whichever shape
/// or marquee was used last, the options show all of them, and the cursor shows whichever is in
/// hand. One table of names, so a tool cannot be one picture in the panel and another under the
/// pointer.
extension CalmTool {
    /// The icon in `design/icons` that stands for this tool. The marquee tools share one glyph
    /// here — the grid shows the family — and are told apart by `ToolIcon.selection`.
    var iconName: String {
        switch self {
        case .pen: return "pen"
        case .eraser: return "eraser"
        case .blur: return "blur"
        case .clone: return "clone"
        case .heal: return "heal"
        case .bucket: return "bucket"
        case .eyedropper: return "eyedropper"
        case .text: return "text"
        case .move: return "move"
        case .transform: return "transform"
        case .line: return "line"
        case .rect: return "shape"
        case .ellipse: return "ellipse"
        case .arrow: return "arrow"
        case .triangle: return "triangle"
        case .pentagon: return "pentagon"
        case .selectRect, .selectEllipse, .selectLasso: return "select-rect"
        case .magicWand: return "magic-wand"
        case .selectColor: return "select-color"
        }
    }

    /// The glyph for one *member* of the marquee family, where `iconName` gives the family's.
    var selectionIconName: String {
        switch self {
        case .selectEllipse: return "select-ellipse"
        case .selectLasso: return "select-lasso"
        case .magicWand: return "magic-wand"
        case .selectColor: return "select-color"
        default: return "select-rect"
        }
    }
}

enum ToolIcon {
    static func tool(_ tool: CalmTool, color: Color) -> some View {
        SvgIcon(name: tool.iconName, color: color)
    }

    /// The marquee tools are the one family whose members do not each have a glyph of their
    /// own in `tool` — a rect, an ellipse and a lasso all select, so they are told apart here.
    static func selection(_ tool: CalmTool, color: Color) -> some View {
        SvgIcon(name: tool.selectionIconName, color: color)
    }

    static func brush(_ brush: CalmBrush, color: Color) -> some View {
        SvgIcon(name: brush.iconName, color: color)
    }
}

extension CalmBrush {
    var iconName: String {
        switch self {
        case .pen: return "pen"
        case .marker: return "marker"
        case .crayon: return "crayon"
        case .airbrush: return "airbrush"
        }
    }
}
