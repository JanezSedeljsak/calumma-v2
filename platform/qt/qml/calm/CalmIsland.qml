import QtQuick

// A panel with the app's one visual signature: `radius.island`, a hairline border, the surface
// colour. Tools, layers, and the color picker are all one of these — see docs/STYLE.md.
Rectangle {
    id: root

    default property alias content: contentHolder.data
    property real padding: theme.spaceXs

    radius: theme.radiusIsland
    color: theme.surface
    border.width: 1
    border.color: theme.islandBorder

    Item {
        id: contentHolder
        anchors.fill: parent
        anchors.margins: root.padding
    }
}
