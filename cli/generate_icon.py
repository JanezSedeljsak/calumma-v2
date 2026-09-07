#!/usr/bin/env python3

from __future__ import annotations

import sys
from pathlib import Path

from constants import DESIGN, MSG_WROTE, ROOT
from PIL import Image, ImageDraw

ICON_SOURCE = DESIGN / "icon.png"
ICON_OUTPUT = DESIGN / "icon-rounded.png"
ICON_SIZE = 256
ICON_RADIUS = 48
COMPOSE_SIZE = 1024
BACKGROUND = (0x22, 0x22, 0x22, 255)
PAD_FRACTION = 0.18


def rounded_mask(size: int, radius: int) -> Image.Image:
    mask = Image.new("L", (size, size), 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, size - 1, size - 1), radius=radius, fill=255)
    return mask


def compose_mark(
    mark: Image.Image, size: int, background: tuple[int, int, int, int], pad_fraction: float
) -> Image.Image:
    canvas = Image.new("RGBA", (size, size), background)
    max_dim = size * (1 - pad_fraction * 2)
    scale = min(max_dim / mark.width, max_dim / mark.height)
    new_w = round(mark.width * scale)
    new_h = round(mark.height * scale)
    resized = mark.resize((new_w, new_h), Image.Resampling.LANCZOS)
    ox = (size - new_w) // 2
    oy = (size - new_h) // 2
    canvas.alpha_composite(resized, (ox, oy))
    return canvas


def generate_icon(
    source: Path = ICON_SOURCE,
    dest: Path = ICON_OUTPUT,
    size: int = ICON_SIZE,
    radius: int = ICON_RADIUS,
) -> Path:
    with Image.open(source) as img:
        mark = img.convert("RGBA")

    composed = compose_mark(mark, COMPOSE_SIZE, BACKGROUND, PAD_FRACTION)
    resized = composed.resize((size, size), Image.Resampling.LANCZOS)

    mask = rounded_mask(size, radius)
    rounded = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    rounded.paste(resized, (0, 0), mask)

    dest.parent.mkdir(parents=True, exist_ok=True)
    rounded.save(dest, format="PNG", optimize=True, compress_level=9)
    return dest


def main() -> int:
    if not ICON_SOURCE.is_file():
        print(f"{ICON_SOURCE.relative_to(ROOT)} not found", file=sys.stderr)
        return 1

    dest = generate_icon()
    size_kb = dest.stat().st_size // 1024
    print(
        f"{MSG_WROTE} {dest.relative_to(ROOT)} ({ICON_SIZE}x{ICON_SIZE}, {size_kb} KB, {ICON_RADIUS}px radius)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
