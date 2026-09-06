import QtQuick
import QtQuick.Controls.Basic

// Root of the QML tree, and — critically — the `QQuickWindow` `main.cpp` reparents the native
// board under (`board.embedUnder(root)`, called indirectly through `BoardHost`'s
// `Component.onCompleted` in Editor.qml). `controller`, `theme` and `l10n` arrive as context
// properties set on the engine's root context before this file loads; nothing here constructs
// them.
ApplicationWindow {
    id: window
    width: 1280
    height: 800
    minimumWidth: 760
    minimumHeight: 520
    visible: true
    title: l10n.t("brand")
    color: theme.bg

    menuBar: MenuBar {
        Menu {
            title: l10n.t("newProjectMenu")
            // Disabled on Landing, same as CalummaApp.swift's `.disabled(app.showLanding)`:
            // Landing already *is* the New Project form, so there is nothing this would open
            // that is not already on screen. Editor.qml owns the popup this opens.
            MenuItem {
                text: l10n.t("newProjectMenu")
                enabled: !controller.showLanding
                onTriggered: editorLoader.item.openNewProjectPopup()
            }
        }
        Menu {
            title: l10n.t("undo")
            MenuItem {
                text: l10n.t("undo")
                enabled: controller.canUndo
                onTriggered: controller.undo()
            }
            MenuItem {
                text: l10n.t("redo")
                enabled: controller.canRedo
                onTriggered: controller.redo()
            }
        }
        Menu {
            title: l10n.t("settings")
            MenuItem {
                text: l10n.t("settings")
                onTriggered: settingsDialog.open()
            }
        }
        Menu {
            title: l10n.t("boardMenu")
            MenuItem {
                text: l10n.t("fitToView")
                enabled: !controller.showLanding
                onTriggered: controller.fit()
            }
        }
    }

    Loader {
        id: editorLoader
        anchors.fill: parent
        sourceComponent: controller.showLanding ? landingComponent : editorComponent
    }

    Component {
        id: landingComponent
        Landing {}
    }
    Component {
        id: editorComponent
        Editor {}
    }

    SettingsView {
        id: settingsDialog
    }

    ToastBanner {
        anchors.bottom: parent.bottom
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottomMargin: theme.spaceLg
    }
}
