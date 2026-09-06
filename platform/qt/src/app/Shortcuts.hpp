#pragma once

#include <Calumma.hpp>

#include <array>
#include <cctype>
#include <optional>
#include <string_view>

// The one table of tool shortcuts, ported byte-for-byte from `ToolLabels.swift`'s `byKey` so
// the key that switches a tool cannot drift between the two shells. Chords that are not a bare
// letter (Ctrl+T, Ctrl+Z, …) are not tool keys — they are actions with a tool as a side effect,
// and they live in `BoardWindow`'s key handling instead. Document every entry in
// `docs/FLOW.md` → Shortcuts in the same change that touches this table.
//
// `CalmTool` itself comes from `platform/shared/Calumma.hpp` — the canonical wire-value enum
// both shells import, rather than a locally hand-duplicated one (Swift used to hand-duplicate
// it too; both now point at the one declaration). Only the lookup logic below is Qt-shell-
// specific, which is why it is still Qt-free and `qt-smoke` can pin it without a window.
namespace calumma::shortcuts {

struct ToolKey {
    char key;
    CalmTool tool;
};

inline constexpr std::array<ToolKey, 18> kToolKeys{{
    {'p', CalmToolPen},
    {'l', CalmToolLine},
    {'r', CalmToolRect},
    {'o', CalmToolEllipse},
    {'a', CalmToolArrow},
    {'3', CalmToolTriangle},
    {'5', CalmToolPentagon},
    {'t', CalmToolText},
    {'e', CalmToolEraser},
    {'u', CalmToolBlur},
    {'c', CalmToolClone},
    {'h', CalmToolHeal},
    {'g', CalmToolBucket},
    {'i', CalmToolEyedropper},
    {'m', CalmToolSelectRect},
    {'w', CalmToolMagicWand},
    {'v', CalmToolMove},
    // Note: 'k' for Crop is intentionally absent from `docs/FLOW.md`'s printed shortcut list
    // in the Swift app history at the time this table was ported; kept here to match
    // `ToolLabels.swift`'s live table. Re-check both if the two ever disagree.
    {'k', CalmToolCrop},
}};

// `key` is expected lowercased already, matching `charactersIgnoringModifiers` on the Swift
// side and `QKeyEvent::text()` lowercased on this one.
inline std::optional<CalmTool> toolForKey(char key) {
    for (const ToolKey &entry : kToolKeys) {
        if (entry.key == key) {
            return entry.tool;
        }
    }
    return std::nullopt;
}

// The marquee family (SelectRect/SelectEllipse/SelectLasso) all answer 'm' — which member 'm'
// actually switches to is whichever was last used, tracked by the caller (`AppState`'s
// last-select-tool is the engine's `CalmState.last_select_tool`, not held here). MagicWand and
// SelectColor have their own keys and are not part of the family.
inline bool isMarqueeFamily(CalmTool tool) {
    return tool == CalmToolSelectRect || tool == CalmToolSelectEllipse ||
           tool == CalmToolSelectLasso;
}

// The key printed in a tooltip next to a tool's name. Transform and SelectColor are chords
// rather than a bare key, so they are not in the table above; everything else is inverted from
// it, one key per tool.
inline std::optional<char> keyForTool(CalmTool tool) {
    if (tool == CalmToolTransform || tool == CalmToolSelectColor) {
        return std::nullopt;
    }
    const CalmTool family = isMarqueeFamily(tool) ? CalmToolSelectRect : tool;
    for (const ToolKey &entry : kToolKeys) {
        if (entry.tool == family) {
            return entry.key;
        }
    }
    return std::nullopt;
}

// The label a tooltip prints beside a tool's name — `shortcutLabel` in `ToolLabels.swift`.
// Empty when the tool has no bare-letter key (blocked tools print no shortcut at all, which is
// the caller's job to gate, not this function's).
inline std::string_view shortcutLabel(CalmTool tool, std::array<char, 3> &scratch) {
    if (tool == CalmToolTransform) {
        return "⌘T";
    }
    if (tool == CalmToolSelectColor) {
        return "⇧W";
    }
    const std::optional<char> key = keyForTool(tool);
    if (!key) {
        return {};
    }
    scratch[0] = static_cast<char>(std::toupper(static_cast<unsigned char>(*key)));
    scratch[1] = '\0';
    return std::string_view(scratch.data(), 1);
}

}  // namespace calumma::shortcuts
