import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import "calm"

// The left island: the tool grid, then whichever options the active tool actually takes — asked
// of the engine (`controller.toolTakes*`), never a switch statement guessing at it. Mirrors
// `ToolsPanel.swift` + `ToolOptions.swift`, trimmed to the tools wired into `Shortcuts.h` so
// far: no shape/select sub-menus, no AI section (no platform ops on this shell — see
// `EngineOps.cpp`).
CalmIsland {
    id: root
    padding: theme.spaceXs

    // Tool rows: {wire value, icon name}. Wire values match `shortcuts::Tool` in Shortcuts.h.
    readonly property var tools: [
        { tool: 15, icon: "move" },
        { tool: 6, icon: "select-rect" },
        { tool: 0, icon: "pen" },
        { tool: 5, icon: "eraser" },
        { tool: 16, icon: "blur" },
        { tool: 19, icon: "clone" },
        { tool: 20, icon: "heal" },
        { tool: 2, icon: "shape" },
        { tool: 9, icon: "bucket" },
        { tool: 11, icon: "eyedropper" },
        { tool: 14, icon: "text" },
        { tool: 21, icon: "crop" },
    ]

    ColumnLayout {
        anchors.fill: parent
        spacing: theme.spaceSm

        GridLayout {
            Layout.fillWidth: true
            columns: 2
            rowSpacing: theme.spaceXs
            columnSpacing: theme.spaceXs

            Repeater {
                model: root.tools
                delegate: CalmToolButton {
                    required property var modelData
                    iconName: modelData.icon
                    selected: controller.tool === modelData.tool
                    tooltipText: modelData.icon
                    onClicked: controller.tool = modelData.tool
                }
            }
        }

        CalmDivider {}

        ColumnLayout {
            Layout.fillWidth: true
            spacing: theme.spaceXs
            visible: controller.toolTakesBrushSize(controller.tool)
            CalmText { variant: "label"; text: l10n.t("brushSize") + " " + Math.round(controller.brushSize) }
            Slider {
                Layout.fillWidth: true
                from: controller.brushSizeMin()
                to: controller.brushSizeMax()
                value: controller.brushSize
                onMoved: controller.brushSize = value
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            spacing: theme.spaceXs
            visible: controller.toolTakesInkOpacity(controller.tool)
            CalmText { variant: "label"; text: l10n.t("inkOpacity") }
            Slider {
                Layout.fillWidth: true
                from: controller.inkOpacityMin()
                to: controller.inkOpacityMax()
                value: controller.inkOpacity
                onMoved: controller.inkOpacity = value
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            spacing: theme.spaceXs
            visible: controller.toolTakesTolerance(controller.tool)
            CalmText { variant: "label"; text: l10n.t("tolerance") + " " + controller.tolerance }
            Slider {
                Layout.fillWidth: true
                from: 0
                to: controller.toleranceMax()
                stepSize: 1
                value: controller.tolerance
                onMoved: controller.tolerance = Math.round(value)
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            spacing: theme.spaceXs
            visible: controller.toolTakesBlurStrength(controller.tool)
            CalmText { variant: "label"; text: l10n.t("blurStrength") }
            Slider {
                Layout.fillWidth: true
                from: controller.blurStrengthMin()
                to: controller.blurStrengthMax()
                value: controller.blurStrength
                onMoved: controller.blurStrength = value
            }
        }

        RowLayout {
            Layout.fillWidth: true
            visible: controller.toolTakesFill(controller.tool)
            spacing: theme.spaceSm
            CalmText { text: "F"; Layout.fillWidth: true }
            Switch {
                checked: controller.shapeFill
                onToggled: controller.shapeFill = checked
            }
        }
        RowLayout {
            Layout.fillWidth: true
            visible: controller.toolTakesFill(controller.tool)
            spacing: theme.spaceSm
            CalmText { text: "S"; Layout.fillWidth: true }
            Switch {
                checked: controller.shapeStroke
                onToggled: controller.shapeStroke = checked
            }
        }

        CalmDivider {}

        CalmText { variant: "label"; text: l10n.t("color") }
        Rectangle {
            width: 28; height: 28; radius: 14
            color: controller.color
            border.width: 1
            border.color: theme.controlBorder
            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onClicked: colorPopup.visible = !colorPopup.visible
            }
        }

        Item { Layout.fillHeight: true }
    }

    QuickColorPicker {
        id: colorPopup
        visible: false
        x: root.width + theme.spaceSm
        y: 0
        z: 10
    }
}
