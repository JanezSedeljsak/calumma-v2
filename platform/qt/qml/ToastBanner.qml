import QtQuick
import "calm"

// `CalmToastView.swift`: a transient status banner, gone on its own — `AppController` holds at
// most one at a time and times it out itself, so this only ever shows or hides what it is told.
Rectangle {
    id: root
    visible: controller.toastVisible
    opacity: controller.toastVisible ? 1.0 : 0.0
    Behavior on opacity { NumberAnimation { duration: 150 } }

    width: label.implicitWidth + theme.spaceLg * 2
    height: 40
    radius: theme.radiusLg
    color: theme.surface
    border.width: 1
    border.color: controller.toastIsError ? theme.danger : theme.accentTeal

    CalmText {
        id: label
        anchors.centerIn: parent
        text: (controller.toastIsError ? "⚠ " : "✓ ") + controller.toastText
        strong: true
    }

    MouseArea {
        anchors.fill: parent
        onClicked: controller.dismissToast()
    }
}
