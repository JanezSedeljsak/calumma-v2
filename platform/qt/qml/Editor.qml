import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import "calm"

// `EditorView.swift`'s three-column layout: tools | canvas | layers, with the project tab row
// on top and the zoom pill floating over the bottom-trailing corner of the board — minus the
// ruler, guides, text options and AI section, none of which are wired into this shell yet.
Rectangle {
    id: root
    color: theme.desk

    // Called from Main.qml's File menu (`⌘N`, matching CalummaApp.swift's `CommandGroup(replacing:
    // .newItem)`), which is disabled while on Landing — Landing already is the New Project form.
    function openNewProjectPopup() { newProjectPopup.open() }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: theme.spaceSm
        spacing: theme.spaceSm

        // --- Project tabs -------------------------------------------------------------------
        RowLayout {
            Layout.fillWidth: true
            Layout.preferredHeight: 28
            spacing: 0

            Rectangle {
                Layout.fillWidth: true
                Layout.fillHeight: true
                radius: 14
                color: theme.surface
                clip: true

                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: theme.spaceXs
                    anchors.rightMargin: theme.spaceXs
                    spacing: 2

                    Repeater {
                        model: controller.openTabs
                        delegate: Rectangle {
                            id: tab
                            required property string projectId
                            required property string name
                            required property int accent
                            Layout.preferredWidth: label.implicitWidth + 56
                            Layout.fillHeight: true
                            color: controller.activeProjectId === projectId ? theme.surfaceHover
                                                                             : "transparent"
                            radius: theme.radiusSm

                            RowLayout {
                                anchors.fill: parent
                                anchors.margins: theme.spaceXs
                                spacing: theme.spaceXs
                                Rectangle {
                                    width: 8; height: 8; radius: 4
                                    color: theme.packedToColor(tab.accent)
                                }
                                CalmText {
                                    id: label
                                    text: tab.name
                                    strong: controller.activeProjectId === tab.projectId
                                    Layout.fillWidth: true
                                    MouseArea {
                                        anchors.fill: parent
                                        cursorShape: Qt.PointingHandCursor
                                        onClicked: controller.switchToProject(tab.projectId)
                                    }
                                }
                                CalmText {
                                    text: "×"
                                    MouseArea {
                                        anchors.fill: parent
                                        cursorShape: Qt.PointingHandCursor
                                        onClicked: controller.closeProjectTab(tab.projectId)
                                    }
                                }
                            }
                        }
                    }

                    CalmToolButton {
                        iconName: "plus"
                        tooltipText: l10n.t("newProject")
                        onClicked: newProjectPopup.open()
                    }
                }
            }
        }

        // --- Tools | board | layers ----------------------------------------------------------
        RowLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: theme.spaceSm

            ToolsPanel {
                Layout.preferredWidth: 92
                Layout.fillHeight: true
            }

            Item {
                id: boardArea
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true

                // The one placeholder the native board is positioned under —
                // `AppController::attachBoardHost` reads this item's geometry and keeps the
                // reparented `BoardWindow` matched to it. See the plan's phase 5 notes: this
                // exact mechanism has never been run against a real Qt build.
                Item {
                    id: boardHost
                    anchors.fill: parent
                    Component.onCompleted: controller.attachBoardHost(boardHost)
                }

                CalmIsland {
                    id: zoomPill
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    anchors.margins: theme.spaceSm
                    padding: theme.spaceSm
                    width: 260
                    implicitHeight: 40

                    RowLayout {
                        anchors.fill: parent
                        spacing: theme.spaceSm
                        CalmText {
                            text: "−"
                            MouseArea {
                                anchors.fill: parent
                                cursorShape: Qt.PointingHandCursor
                                onClicked: controller.stepZoom(false)
                            }
                        }
                        Slider {
                            Layout.fillWidth: true
                            from: 0
                            to: 1
                            value: controller.zoomUnit
                            onMoved: controller.setZoomUnit(value)
                        }
                        CalmText {
                            text: "+"
                            MouseArea {
                                anchors.fill: parent
                                cursorShape: Qt.PointingHandCursor
                                onClicked: controller.stepZoom(true)
                            }
                        }
                        CalmText {
                            variant: "muted"
                            text: Math.round(controller.zoom * 100) + "%"
                        }
                        CalmToolButton {
                            iconName: "fit-to-view"
                            selected: controller.isFit
                            tooltipText: l10n.t("fitToView")
                            onClicked: controller.fit()
                        }
                    }
                }
            }

            LayersPanel {
                Layout.preferredWidth: 220
                Layout.fillHeight: true
            }
        }
    }

    Popup {
        id: newProjectPopup
        modal: true
        width: 760
        height: 480
        anchors.centerIn: Overlay.overlay
        background: Rectangle {
            color: theme.surface
            radius: theme.radiusWindow
            border.width: 1
            border.color: theme.islandBorder
        }
        NewProjectForm {
            anchors.fill: parent
            anchors.margins: theme.spaceLg
        }
        Connections {
            target: controller
            function onNavigationChanged() { newProjectPopup.close() }
        }
    }
}
