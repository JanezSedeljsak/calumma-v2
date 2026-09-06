import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import "calm"

// `SettingsView.swift`: theme (a plain Light/Dark toggle — there is no third "System" option to
// offer, see AppState::Theme) and language. `AppLanguage` only has one member (`en`) at the
// point this was written, so the language list is one row rather than a real picker.
Popup {
    id: root
    modal: true
    focus: true
    width: 360
    height: 340
    anchors.centerIn: Overlay.overlay
    background: Rectangle {
        color: theme.surface
        radius: theme.radiusWindow
        border.width: 1
        border.color: theme.islandBorder
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: theme.spaceXl
        spacing: theme.spaceXl

        RowLayout {
            Layout.fillWidth: true
            CalmText { variant: "title"; strong: true; text: l10n.t("settings"); Layout.fillWidth: true }
            CalmText {
                text: "✕"
                MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.close() }
            }
        }

        ColumnLayout {
            spacing: theme.spaceSm
            CalmText { variant: "label"; text: l10n.t("theme") }
            RowLayout {
                spacing: theme.spaceSm
                CalmButton {
                    text: l10n.t("themeLight")
                    onClicked: controller.themeMode = 0
                    opacity: controller.themeMode === 0 ? 1.0 : 0.6
                }
                CalmButton {
                    text: l10n.t("themeDark")
                    onClicked: controller.themeMode = 1
                    opacity: controller.themeMode === 1 ? 1.0 : 0.6
                }
            }
        }

        ColumnLayout {
            spacing: theme.spaceSm
            CalmText { variant: "label"; text: l10n.t("language") }
            Rectangle {
                Layout.fillWidth: true
                height: theme.controlHeight
                radius: theme.radiusSm
                color: theme.surfaceHover
                CalmText {
                    anchors.left: parent.left
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.leftMargin: theme.spaceMd
                    text: "English"
                    strong: true
                }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            CalmText { variant: "label"; text: l10n.t("memoryUsed") }
            Item { Layout.fillWidth: true }
            CalmText { variant: "muted"; text: controller.memoryLabel() }
        }

        Item { Layout.fillHeight: true }
    }
}
