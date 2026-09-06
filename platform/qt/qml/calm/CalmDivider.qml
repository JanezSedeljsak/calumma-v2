import QtQuick
import QtQuick.Layouts

// A one-pixel rule between sections of an island — `CalmDivider` in Components.swift.
Rectangle {
    Layout.fillWidth: true
    height: 1
    color: theme.islandBorder
}
