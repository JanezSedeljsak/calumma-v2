import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import "calm"

// The right island: the layer stack, top row first (`LayerListModel` already reverses it),
// with add/duplicate/merge/delete and the two per-row toggles every row always shows. No
// per-row opacity slider or blend-mode menu yet — `LayerSettingsCard.swift`'s popover is not
// ported; double-clicking a row edits its name, which is the one inline edit this pass has.
CalmIsland {
    id: root
    padding: theme.spaceXs

    ColumnLayout {
        anchors.fill: parent
        spacing: theme.spaceSm

        RowLayout {
            Layout.fillWidth: true
            CalmText { variant: "label"; text: l10n.t("layers"); Layout.fillWidth: true }
            CalmToolButton {
                iconName: "plus"
                tooltipText: l10n.t("addLayer")
                onClicked: controller.addLayer()
            }
        }

        ListView {
            id: list
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: controller.layers
            spacing: 2
            currentIndex: controller.activeLayerRow

            delegate: Rectangle {
                id: row
                required property int index
                required property string name
                required property bool isVisible
                required property bool isLocked
                required property bool isPaper
                property bool renaming: false

                width: ListView.view.width
                height: 34
                radius: theme.radiusSm
                color: index === controller.activeLayerRow ? theme.surfaceHover
                      : rowMouse.containsMouse ? theme.surfaceHover : "transparent"

                RowLayout {
                    anchors.fill: parent
                    anchors.margins: theme.spaceXs
                    spacing: theme.spaceXs

                    CalmToolButton {
                        iconName: row.isVisible ? "eye-open" : "eye-closed"
                        implicitWidth: 24
                        implicitHeight: 24
                        onClicked: controller.setLayerRowVisible(row.index, !row.isVisible)
                    }
                    CalmToolButton {
                        iconName: row.isLocked ? "lock-closed" : "lock-open"
                        implicitWidth: 24
                        implicitHeight: 24
                        visible: !row.isPaper
                        onClicked: controller.setLayerRowLocked(row.index, !row.isLocked)
                    }

                    CalmField {
                        Layout.fillWidth: true
                        visible: row.renaming
                        text: row.name
                        onAccepted: {
                            controller.setLayerRowName(row.index, text)
                            row.renaming = false
                        }
                    }
                    CalmText {
                        Layout.fillWidth: true
                        visible: !row.renaming
                        text: row.name
                        strong: index === controller.activeLayerRow
                    }
                }

                MouseArea {
                    id: rowMouse
                    anchors.fill: parent
                    hoverEnabled: true
                    onClicked: controller.setActiveLayerRow(row.index)
                    onDoubleClicked: if (!row.isPaper) row.renaming = true
                }
            }
        }

        CalmDivider {}

        RowLayout {
            Layout.fillWidth: true
            spacing: theme.spaceXs
            CalmToolButton {
                iconName: "copy"
                tooltipText: l10n.t("deleteLayer")
                enabled: controller.layers.count > 0
                onClicked: controller.duplicateLayerRow(controller.activeLayerRow)
            }
            CalmToolButton {
                iconName: "trash"
                tooltipText: l10n.t("deleteLayer")
                enabled: controller.layers.count > 1
                onClicked: controller.removeLayerRow(controller.activeLayerRow)
            }
            Item { Layout.fillWidth: true }
            CalmToolButton {
                iconName: "more"
                tooltipText: l10n.t("mergeLayerDown")
                enabled: controller.layerRowCanMergeDown(controller.activeLayerRow)
                onClicked: controller.mergeLayerRowDown(controller.activeLayerRow)
            }
        }
    }
}
