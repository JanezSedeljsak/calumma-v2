import QtQuick
import QtQuick.Controls.Basic

// One tool-island button: an icon, a selected ring, a tooltip that carries either the tool's
// name+key or the reason it is blocked — `ToolLabels.swift`'s `toolTooltip`/`toolShortcut`,
// folded into one string here since QML has nowhere else to put a second line cheaply.
Rectangle {
    id: root

    property string iconName: "pen"
    property bool selected: false
    // `enabled` is not declared here — see the identical note in CalmButton.qml. It is
    // `Item`'s own property, used below exactly as if it were a custom one.
    property string tooltipText: ""
    signal clicked()

    width: 36
    height: 36
    radius: theme.radiusMd
    color: selected ? theme.surfaceHover : "transparent"
    border.width: selected ? 1 : 0
    border.color: theme.controlFocusBorder
    opacity: enabled ? 1.0 : 0.38

    ToolIcon {
        anchors.centerIn: parent
        name: root.iconName
        color: root.selected ? theme.accentTeal : theme.textMuted
        iconSize: 18
    }

    MouseArea {
        id: mouse
        anchors.fill: parent
        hoverEnabled: true
        enabled: root.enabled
        cursorShape: Qt.PointingHandCursor
        onClicked: root.clicked()

        ToolTip.visible: containsMouse && root.tooltipText.length > 0
        ToolTip.text: root.tooltipText
        ToolTip.delay: 450
    }
}
