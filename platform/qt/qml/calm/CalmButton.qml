import QtQuick
import QtQuick.Controls.Basic

// A plain and an accent variant, the two button styles Components.swift offers
// (`CalmPlainButton` / `CalmAccentButton`). `accent: true` is the one styled as a call to
// action — Create, in the new-project form.
Rectangle {
    id: root

    property string text: ""
    property bool accent: false
    // `enabled` is not declared here — `Item` already has one, and QML refuses to redeclare a
    // property that collides with an inherited one (the same trap `visible`/`color`/`x` are).
    // `root.enabled` below is that inherited property; callers set it exactly as if it were ours.
    signal clicked()

    implicitWidth: label.implicitWidth + theme.spaceLg * 2
    implicitHeight: theme.controlHeight
    radius: theme.radiusMd
    color: accent ? theme.accentTeal : (mouse.containsMouse ? theme.surfaceHover : "transparent")
    border.width: accent ? 0 : 1
    border.color: theme.controlBorder
    opacity: enabled ? 1.0 : 0.5

    CalmText {
        id: label
        anchors.centerIn: parent
        text: root.text
        variant: "label"
        strong: true
        color: root.accent ? "#ffffff" : theme.text
    }

    MouseArea {
        id: mouse
        anchors.fill: parent
        hoverEnabled: true
        enabled: root.enabled
        cursorShape: Qt.PointingHandCursor
        onClicked: root.clicked()
    }
}
