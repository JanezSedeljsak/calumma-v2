import SwiftUI

enum AppIcon {
    static func settings(color: Color) -> some View {
        SvgIcon(name: "settings", color: color)
    }

    static func plus(color: Color, size: CGFloat = 18) -> some View {
        SvgIcon(name: "plus", color: color, size: size)
    }

    static func pen(color: Color) -> some View {
        SvgIcon(name: "pen", color: color)
    }

    static func shape(color: Color) -> some View {
        SvgIcon(name: "shape", color: color)
    }

    static func image(color: Color) -> some View {
        SvgIcon(name: "image", color: color)
    }

    static func eraser(color: Color) -> some View {
        SvgIcon(name: "eraser", color: color)
    }

    static func fitToView(color: Color) -> some View {
        SvgIcon(name: "fit-to-view", color: color)
    }

    static func line(color: Color) -> some View {
        SvgIcon(name: "line", color: color)
    }

    static func ellipse(color: Color) -> some View {
        SvgIcon(name: "ellipse", color: color)
    }

    static func arrow(color: Color) -> some View {
        SvgIcon(name: "arrow", color: color)
    }

    static func triangle(color: Color) -> some View {
        SvgIcon(name: "triangle", color: color)
    }

    static func pentagon(color: Color) -> some View {
        SvgIcon(name: "pentagon", color: color)
    }

    static func ai(color: Color) -> some View {
        SvgIcon(name: "ai", color: color)
    }

    static func eye(color: Color, open: Bool = true) -> some View {
        SvgIcon(name: open ? "eye-open" : "eye-closed", color: color)
    }

    static func trash(color: Color) -> some View {
        SvgIcon(name: "trash", color: color)
    }

    static func selectRect(color: Color) -> some View {
        SvgIcon(name: "select-rect", color: color)
    }

    static func selectEllipse(color: Color) -> some View {
        SvgIcon(name: "select-ellipse", color: color)
    }

    static func selectLasso(color: Color) -> some View {
        SvgIcon(name: "select-lasso", color: color)
    }

    static func export(color: Color) -> some View {
        SvgIcon(name: "export", color: color)
    }

    static func bucket(color: Color) -> some View {
        SvgIcon(name: "bucket", color: color)
    }

    static func copyIcon(color: Color) -> some View {
        SvgIcon(name: "copy", color: color)
    }

    static func adjust(color: Color) -> some View {
        SvgIcon(name: "adjust", color: color)
    }

    static func more(color: Color) -> some View {
        SvgIcon(name: "more", color: color)
    }

    static func transform(color: Color) -> some View {
        SvgIcon(name: "transform", color: color)
    }

    static func eyedropper(color: Color) -> some View {
        SvgIcon(name: "eyedropper", color: color)
    }

    static func text(color: Color) -> some View {
        SvgIcon(name: "text", color: color)
    }

    static func move(color: Color) -> some View {
        SvgIcon(name: "move", color: color)
    }
}
