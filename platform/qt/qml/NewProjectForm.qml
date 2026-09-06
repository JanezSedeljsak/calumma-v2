import QtQuick
import QtQuick.Layouts
import "calm"

// The create-a-project form: name, accent, size, presets and recents — `NewProjectView.swift`'s
// `createForm` + `presetsColumn` + `recentsColumn`, minus the paste-artwork island (drop/paste
// import is phase 6, not built on either shell path yet here). Used both as the whole Landing
// screen and, later, inside a modal for "New Project" while a project is already open.
Item {
    id: root

    property color accent: "#3aa6a6"
    property bool colorPickerOpen: false

    Component.onCompleted: controller.refreshRecents()

    ColumnLayout {
        anchors.fill: parent
        spacing: theme.spaceLg

        RowLayout {
            Layout.fillWidth: true
            spacing: theme.spaceSm
            CalmText { variant: "brand"; text: l10n.t("brand") }
            CalmText { variant: "eyebrow"; text: l10n.t("tagline"); color: theme.textMuted }
            Item { Layout.fillWidth: true }
        }

        ColumnLayout {
            spacing: theme.spaceXs
            CalmText { variant: "label"; text: l10n.t("projectName") }
            RowLayout {
                spacing: theme.spaceSm
                Rectangle {
                    width: 16; height: 16; radius: 8
                    color: root.accent
                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: root.colorPickerOpen = !root.colorPickerOpen
                    }
                }
                CalmField {
                    id: nameField
                    Layout.preferredWidth: 240
                    text: l10n.t("newProject")
                    onAccepted: createClicked()
                }
            }
            CalmPaletteRow {
                visible: root.colorPickerOpen
                selected: root.accent
                onPicked: (c) => { root.accent = c; root.colorPickerOpen = false }
            }
        }

        ColumnLayout {
            spacing: theme.spaceXs
            CalmText { variant: "label"; text: l10n.t("resolution") }
            RowLayout {
                spacing: theme.spaceSm
                CalmField { id: widthField; Layout.preferredWidth: 72; text: "1280" }
                CalmText { text: "×" }
                CalmField { id: heightField; Layout.preferredWidth: 72; text: "720" }
                CalmButton {
                    text: l10n.t("create")
                    accent: true
                    onClicked: createClicked()
                }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: theme.spaceLg

            // Presets
            ColumnLayout {
                Layout.preferredWidth: 260
                Layout.fillHeight: true
                spacing: theme.spaceSm
                CalmText { variant: "label"; text: l10n.t("presets"); color: theme.accentTeal }
                Repeater {
                    model: theme.presets()
                    delegate: Rectangle {
                        required property var modelData
                        Layout.fillWidth: true
                        height: 44
                        radius: theme.radiusSm
                        color: presetMouse.containsMouse ? theme.surfaceHover : "transparent"
                        RowLayout {
                            anchors.fill: parent
                            anchors.margins: theme.spaceSm
                            CalmText { text: modelData.label; Layout.fillWidth: true }
                            CalmText {
                                variant: "muted"
                                text: modelData.width + " × " + modelData.height
                            }
                        }
                        MouseArea {
                            id: presetMouse
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: controller.createProject(
                                modelData.label, modelData.width, modelData.height,
                                theme.colorToPacked(root.accent))
                        }
                    }
                }
            }

            // Recents
            ColumnLayout {
                Layout.fillWidth: true
                Layout.fillHeight: true
                spacing: theme.spaceSm
                RowLayout {
                    Layout.fillWidth: true
                    CalmText { variant: "label"; text: l10n.t("recents"); color: theme.accentOrange }
                    Item { Layout.fillWidth: true }
                    CalmText {
                        text: l10n.t("clearAllRecents")
                        color: theme.danger
                        visible: controller.recents.count > 0
                        MouseArea {
                            anchors.fill: parent
                            cursorShape: Qt.PointingHandCursor
                            onClicked: controller.deleteAllRecents()
                        }
                    }
                }
                CalmText {
                    variant: "muted"
                    text: l10n.t("noRecents")
                    visible: controller.recents.count === 0
                }
                ListView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    visible: controller.recents.count > 0
                    model: controller.recents
                    spacing: theme.spaceXs
                    clip: true
                    delegate: Rectangle {
                        required property string projectId
                        required property string name
                        required property int projectWidth
                        required property int projectHeight
                        required property int accent
                        width: ListView.view.width
                        height: 48
                        radius: theme.radiusSm
                        color: recentMouse.containsMouse ? theme.surfaceHover : "transparent"

                        RowLayout {
                            anchors.fill: parent
                            anchors.margins: theme.spaceSm
                            anchors.rightMargin: theme.spaceLg + theme.spaceSm
                            Rectangle {
                                width: 32; height: 32; radius: theme.radiusSm
                                color: theme.packedToColor(accent)
                            }
                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 2
                                CalmText { text: name }
                                CalmText { variant: "muted"; text: projectWidth + " × " + projectHeight }
                            }
                        }
                        MouseArea {
                            id: recentMouse
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: controller.openRecent(projectId)
                        }
                        // Declared last, so it sits on top of `recentMouse` for hit-testing —
                        // the row opens the project, this one square deletes it instead.
                        CalmText {
                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            anchors.rightMargin: theme.spaceSm
                            text: "✕"
                            color: theme.danger
                            MouseArea {
                                anchors.fill: parent
                                anchors.margins: -8
                                cursorShape: Qt.PointingHandCursor
                                onClicked: controller.deleteProject(projectId)
                            }
                        }
                    }
                }
            }
        }
    }

    function createClicked() {
        const w = parseInt(widthField.text, 10) || 1280
        const h = parseInt(heightField.text, 10) || 720
        controller.createProject(nameField.text, w, h, theme.colorToPacked(root.accent))
    }
}
