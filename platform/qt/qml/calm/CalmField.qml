import QtQuick
import QtQuick.Controls.Basic

// A bordered text field matching the app's controls — `CalmField` in Components.swift.
Rectangle {
    id: root

    property alias text: input.text
    signal accepted()

    implicitHeight: theme.controlHeight
    radius: theme.radiusSm
    color: "transparent"
    border.width: 1
    border.color: input.activeFocus ? theme.controlFocusBorder : theme.controlBorder

    TextInput {
        id: input
        anchors.fill: parent
        anchors.leftMargin: theme.spaceSm
        anchors.rightMargin: theme.spaceSm
        verticalAlignment: TextInput.AlignVCenter
        color: theme.text
        font.pixelSize: theme.bodySize
        clip: true
        selectByMouse: true
        onAccepted: root.accepted()
    }
}
