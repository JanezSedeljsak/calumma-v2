#!/usr/bin/env python3

from __future__ import annotations

import sys
from pathlib import Path

from constants import DESIGN, MSG_WROTE, ROOT
from PIL import Image, ImageDraw

ICON_SOURCE = DESIGN / "icon.png"
ICON_OUTPUT = DESIGN / "icon-rounded.png"
ICON_SIZE = 256
COMPOSE_SIZE = 1024
BACKGROUND = (0x22, 0x22, 0x22, 255)
MARK_PAD_FRACTION = 0.18
MARK_SCALE_BOOST = 1.3
MARK_HEIGHT_BOOST = 1.1
BADGE_INSET_FRACTION = 0.065
BADGE_RADIUS_FRACTION = 0.22


def compose_icon(mark: Image.Image, size: int) -> Image.Image:
    canvas = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    margin = round(size * BADGE_INSET_FRACTION)
    badge_side = size - margin * 2
    radius = round(badge_side * BADGE_RADIUS_FRACTION)

    draw = ImageDraw.Draw(canvas)
    draw.rounded_rectangle(
        (margin, margin, margin + badge_side - 1, margin + badge_side - 1),
        radius=radius,
        fill=BACKGROUND,
    )

    max_dim = badge_side * (1 - MARK_PAD_FRACTION * 2)
    scale = min(max_dim / mark.width, max_dim / mark.height) * MARK_SCALE_BOOST
    new_w = round(mark.width * scale)
    new_h = round(mark.height * scale * MARK_HEIGHT_BOOST)
    resized_mark = mark.resize((new_w, new_h), Image.Resampling.LANCZOS)
    ox = (size - new_w) // 2
    oy = (size - new_h) // 2
    canvas.alpha_composite(resized_mark, (ox, oy))
    return canvas


def generate_icon(
    source: Path = ICON_SOURCE, dest: Path = ICON_OUTPUT, size: int = ICON_SIZE
) -> Path:
    with Image.open(source) as img:
        mark = img.convert("RGBA")

    composed = compose_icon(mark, COMPOSE_SIZE)
    resized = composed.resize((size, size), Image.Resampling.LANCZOS)

    dest.parent.mkdir(parents=True, exist_ok=True)
    resized.save(dest, format="PNG", optimize=True, compress_level=9)
    return dest


def main() -> int:
    if not ICON_SOURCE.is_file():
        print(f"{ICON_SOURCE.relative_to(ROOT)} not found", file=sys.stderr)
        return 1

    dest = generate_icon()
    size_kb = dest.stat().st_size // 1024
    print(f"{MSG_WROTE} {dest.relative_to(ROOT)} ({ICON_SIZE}x{ICON_SIZE}, {size_kb} KB)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
