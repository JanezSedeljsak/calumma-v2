import QtQuick
import QtQuick.Layouts

// The project-colour swatch row — `CalmPaletteRow` in Components.swift. `controller.palette()`
// is the engine's own list (`calm_palette_color`); this never invents a colour of its own.
RowLayout {
    id: root

    property color selected: "gray"
    signal picked(color c)

    spacing: theme.spaceSm

    Repeater {
        model: controller.palette()

        delegate: Rectangle {
            required property color modelData
            width: 20
            height: 20
            radius: 10
            color: modelData
            border.width: root.selected === modelData ? 2 : 0
            border.color: theme.text

            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onClicked: root.picked(modelData)
            }
        }
    }
}
