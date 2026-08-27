import SwiftUI

/// Which glyph stands for a tool or a brush. Shared because the tool grid and the tool options
/// below it draw the same families from opposite ends: the grid shows whichever shape or
/// marquee was used last, the options show all of them.
enum ToolIcon {
    @ViewBuilder
    static func tool(_ tool: CalmTool, color: Color) -> some View {
        switch tool {
        case .line: AppIcon.line(color: color)
        case .rect: AppIcon.shape(color: color)
        case .ellipse: AppIcon.ellipse(color: color)
        case .arrow: AppIcon.arrow(color: color)
        case .triangle: AppIcon.triangle(color: color)
        case .pentagon: AppIcon.pentagon(color: color)
        case .pen: AppIcon.pen(color: color)
        case .eraser: AppIcon.eraser(color: color)
        case .bucket: AppIcon.bucket(color: color)
        case .blur: AppIcon.blur(color: color)
        case .eyedropper: AppIcon.eyedropper(color: color)
        case .text: AppIcon.text(color: color)
        case .move: AppIcon.moveIcon(color: color)
        case .transform: AppIcon.transform(color: color)
        case .selectRect, .selectEllipse, .selectLasso: AppIcon.selectRect(color: color)
        case .magicWand: AppIcon.magicWand(color: color)
        }
    }

    /// The marquee tools are the one family whose members do not each have a glyph of their
    /// own in `tool` — a rect, an ellipse and a lasso all select, so they are told apart here.
    @ViewBuilder
    static func selection(_ tool: CalmTool, color: Color) -> some View {
        switch tool {
        case .selectEllipse: AppIcon.selectEllipse(color: color)
        case .selectLasso: AppIcon.selectLasso(color: color)
        case .magicWand: AppIcon.magicWand(color: color)
        default: AppIcon.selectRect(color: color)
        }
    }

    @ViewBuilder
    static func brush(_ brush: CalmBrush, color: Color) -> some View {
        switch brush {
        case .pen: AppIcon.pen(color: color)
        case .marker: AppIcon.marker(color: color)
        case .crayon: AppIcon.crayon(color: color)
        case .airbrush: AppIcon.airbrush(color: color)
        }
    }
}
