import QtQuick
import QtQuick.Layouts
import "calm"

// A simplified `QuickColorPicker.swift`: the project palette plus a hex field, rather than the
// full hue/saturation/brightness square — the two-dimensional picker is real work (a draggable
// SV area, a hue ring) that did not fit this pass. `controller.color` is read and written
// directly, so this never holds a colour of its own to fall out of sync.
CalmIsland {
    id: root
    width: 200
    implicitHeight: content.implicitHeight + spaceLg
    readonly property real spaceLg: theme.spaceLg

    ColumnLayout {
        id: content
        anchors.fill: parent
        spacing: theme.spaceSm

        CalmText { variant: "label"; text: l10n.t("color") }

        Rectangle {
            Layout.fillWidth: true
            height: 32
            radius: theme.radiusSm
            color: controller.color
            border.width: 1
            border.color: theme.controlBorder
        }

        GridLayout {
            Layout.fillWidth: true
            columns: 8
            rowSpacing: theme.spaceXs
            columnSpacing: theme.spaceXs
            Repeater {
                model: controller.palette()
                delegate: Rectangle {
                    required property color modelData
                    width: 18; height: 18; radius: 9
                    color: modelData
                    border.width: controller.color === modelData ? 2 : 0
                    border.color: theme.text
                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: controller.color = modelData
                    }
                }
            }
        }

        CalmField {
            id: hexField
            Layout.fillWidth: true
            text: controller.formatHex(theme.colorToPacked(controller.color))
            onAccepted: {
                const rgb = controller.parseHex(text)
                if (rgb >= 0) {
                    controller.color = theme.packedToColor(rgb)
                }
            }
        }
    }
}
