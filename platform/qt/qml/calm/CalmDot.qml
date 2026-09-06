import QtQuick

// A project's accent colour, as a small filled circle — `CalmDot` in Components.swift, used on
// tab chips and the new-project accent picker. `color` is Rectangle's own property; nothing to
// add beyond a sensible default size and making it round.
Rectangle {
    property real dotSize: 14
    width: dotSize
    height: dotSize
    radius: dotSize / 2
    color: "gray"
}
