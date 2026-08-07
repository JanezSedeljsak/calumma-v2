#!/usr/bin/env python3

from __future__ import annotations

import sys
from pathlib import Path

from _helpers import (
    COLOR_KEYS,
    TOKEN_MODE_DARK,
    TOKEN_MODE_LIGHT,
    TOKENS_SWIFT_OUT,
    accent_pair,
    load_tokens,
    swift_color_lit,
    token_colors,
    token_presets,
    token_radius,
    token_space,
    token_type,
    token_window,
)
from constants import (
    ENCODING_UTF8,
    MSG_WROTE,
    RADIUS_KEYS,
    SPACE_KEYS,
    TYPE_KEYS,
    WINDOW_KEYS,
)


def generate_tokens_swift() -> Path:
    tokens = load_tokens()
    radius = token_radius(tokens)
    space = token_space(tokens)
    typ = token_type(tokens)
    window = token_window(tokens)
    colors = token_colors(tokens)
    presets = token_presets(tokens)
    light = colors[TOKEN_MODE_LIGHT]
    dark = colors[TOKEN_MODE_DARK]

    lines = [
        "import SwiftUI",
        "",
        "enum Tokens {",
        "    enum Radius {",
    ]
    for key in RADIUS_KEYS:
        lines.append(f"        static let {key}: CGFloat = {radius[key]}")
    lines += ["    }", "", "    enum Space {"]
    for key in SPACE_KEYS:
        lines.append(f"        static let {key}: CGFloat = {space[key]}")
    lines += ["    }", "", "    enum TypeSize {"]
    for json_key, swift_name in TYPE_KEYS:
        lines.append(f"        static let {swift_name}: CGFloat = {typ[json_key]}")
    lines += ["    }", "", "    enum Window {"]
    for key in WINDOW_KEYS:
        lines.append(f"        static let {key}: CGFloat = {window[key]}")
    lines += ["    }", "", "    enum Light {"]
    for key in COLOR_KEYS:
        value = light[key]
        assert isinstance(value, str)
        lines.append(swift_color_lit(key, value))
    teal, orange = accent_pair(light)
    lines.append(swift_color_lit("accentTeal", teal))
    lines.append(swift_color_lit("accentOrange", orange))
    lines += ["    }", "", "    enum Dark {"]
    for key in COLOR_KEYS:
        value = dark[key]
        assert isinstance(value, str)
        lines.append(swift_color_lit(key, value))
    teal, orange = accent_pair(dark)
    lines.append(swift_color_lit("accentTeal", teal))
    lines.append(swift_color_lit("accentOrange", orange))
    lines += [
        "    }",
        "",
        "    struct Preset: Identifiable, Hashable {",
        "        let id: String",
        "        let label: String",
        "        let width: Int",
        "        let height: Int",
        "    }",
        "",
        "    static let presets: [Preset] = [",
    ]
    for preset in presets:
        label = str(preset["label"])
        pid = label.lower().replace(" ", "-").replace(":", "")
        lines.append(
            f'        Preset(id: "{pid}", label: "{label}", width: {int(preset["width"])}, height: {int(preset["height"])}),'
        )
    lines += ["    ]", "}", ""]

    TOKENS_SWIFT_OUT.parent.mkdir(parents=True, exist_ok=True)
    TOKENS_SWIFT_OUT.write_text("\n".join(lines), encoding=ENCODING_UTF8)
    return TOKENS_SWIFT_OUT


def main() -> int:
    out = generate_tokens_swift()
    print(f"{MSG_WROTE} {out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
