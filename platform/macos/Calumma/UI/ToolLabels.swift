import Foundation

/// What a tool button says about itself: its name, its key, and — when the active layer refuses
/// it — the refusal in place of both.
extension Engine {
    /// The tool's name, or why it cannot run here. One string, so hovering a dimmed icon
    /// answers the only question anyone has about it.
    func toolTooltip(_ tool: CalmTool, _ l10n: L10nCatalog) -> String {
        toolBlock(tool).reason(l10n) ?? tool.name(l10n)
    }

    /// The key beside that name, and nothing beside a refusal: a shortcut printed next to
    /// "this layer is locked" would read as the way around it.
    func toolShortcut(_ tool: CalmTool) -> String? {
        isBlocked(tool) ? nil : tool.shortcutLabel
    }
}

extension CalmTool {
    func name(_ l10n: L10nCatalog) -> String {
        switch self {
        case .pen: return l10n.toolPen
        case .eraser: return l10n.toolEraser
        case .bucket: return l10n.toolBucket
        case .blur: return l10n.toolBlur
        case .line: return l10n.toolLine
        case .rect: return l10n.toolRect
        case .ellipse: return l10n.toolEllipse
        case .arrow: return l10n.toolArrow
        case .triangle: return l10n.toolTriangle
        case .pentagon: return l10n.toolPentagon
        case .selectRect: return l10n.toolSelectRect
        case .selectEllipse: return l10n.toolSelectEllipse
        case .selectLasso: return l10n.toolSelectLasso
        case .magicWand: return l10n.toolMagicWand
        case .eyedropper: return l10n.toolEyedropper
        case .text: return l10n.toolText
        case .move: return l10n.toolMove
        case .transform: return l10n.toolTransform
        }
    }
}

/// The one table of tool shortcuts. `EditorShortcuts` picks tools through it and the tools
/// panel prints them in tooltips, so the key that switches a tool and the key a tooltip
/// promises cannot drift apart. Chords that are not a bare letter (`⌘T`, `⌘Z`, …) stay in
/// `EditorShortcuts` — they are actions with a tool as a side effect, not tool keys.
extension CalmTool {
    /// Keyed by the character the user types, lowercased, as `charactersIgnoringModifiers`
    /// reports it. Document every entry in `docs/FLOW.md` → Shortcuts in the same change.
    private static let byKey: [String: CalmTool] = [
        "p": .pen,
        "l": .line,
        "r": .rect,
        "o": .ellipse,
        "a": .arrow,
        "3": .triangle,
        "5": .pentagon,
        "t": .text,
        "e": .eraser,
        "u": .blur,
        "g": .bucket,
        "i": .eyedropper,
        "m": .selectRect,
        "w": .magicWand,
    ]

    private static let keyByTool: [CalmTool: String] = Dictionary(
        uniqueKeysWithValues: byKey.map { ($0.value, $0.key) }
    )

    static func forShortcut(_ key: String) -> CalmTool? {
        byKey[key.lowercased()]
    }

    /// `m` names the marquee *family* rather than one member of it, so all three answer with
    /// it — which member the key lands on is whichever was used last (`AppModel.lastSelectTool`).
    /// The magic wand has its own key and is not part of that family.
    private var shortcutFamily: CalmTool {
        isSelection && self != .magicWand ? .selectRect : self
    }

    /// What a tooltip prints beside the tool's name, or nothing for a tool with no key of its
    /// own. Move is the one chord in here: it has no bare letter (`V` is vector mode), and with
    /// transform now coming on with it, `⌘T` is exactly the shortcut that reaches it.
    var shortcutLabel: String? {
        if self == .move || self == .transform { return "⌘T" }
        return CalmTool.keyByTool[shortcutFamily]?.uppercased()
    }
}
