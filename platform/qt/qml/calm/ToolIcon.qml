import QtQuick
import QtQuick.Effects

// One SVG from design/icons/, tinted to whatever colour the caller wants — the Qt counterpart
// of `SvgIcon.swift`'s `NSImage.isTemplate`, which recolours the same way at draw time instead
// of needing a colour variant baked into the file. Every tool and chrome icon in
// design/icons/*.svg is a single flat shape, so `MultiEffect.colorization: 1.0` (full tint) is
// always the right amount rather than a blend with the source's own colour.
Item {
    id: root

    property string name: "pen"
    property color color: "black"
    property real iconSize: 18

    implicitWidth: iconSize
    implicitHeight: iconSize

    Image {
        id: source
        anchors.fill: parent
        source: "qrc:/icons/" + root.name + ".svg"
        sourceSize.width: root.iconSize
        sourceSize.height: root.iconSize
        smooth: true
        antialiasing: true
        visible: false
        fillMode: Image.PreserveAspectFit
    }

    MultiEffect {
        anchors.fill: parent
        source: source
        colorization: 1.0
        colorizationColor: root.color
        brightness: 0.0
    }
}
