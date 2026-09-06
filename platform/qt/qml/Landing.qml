import QtQuick

// `NewProjectView.swift` with `isLanding: true` — the whole window before any project is open.
Rectangle {
    color: theme.bg

    NewProjectForm {
        anchors.fill: parent
        anchors.margins: theme.spaceLg
    }
}
