import QtQuick

// The one place text styling happens — `CalmText.*` on the Swift side. `variant` picks the
// token-driven size/weight/colour combination rather than each caller choosing its own.
Text {
    id: root

    property string variant: "body"  // body | label | title | brand | muted | eyebrow
    property bool strong: false

    color: variant === "muted" ? theme.textMuted : theme.text
    font.pixelSize: variant === "label" || variant === "eyebrow" ? theme.labelSize
                   : variant === "title" ? theme.titleSize
                   : variant === "brand" ? theme.brandSize
                   : theme.bodySize
    font.weight: strong || variant === "title" || variant === "brand" ? Font.DemiBold
                                                                       : Font.Normal
    font.letterSpacing: variant === "label" || variant === "eyebrow" ? 0.6 : 0
    elide: Text.ElideRight
}
