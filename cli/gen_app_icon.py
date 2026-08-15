#!/usr/bin/env python3

from __future__ import annotations

import sys
from pathlib import Path

from constants import APPICON_DIR, APPICON_SIZES, ICON_MASTER, MSG_NO_ICON_MASTER, MSG_WROTE
from PIL import Image


def write_png(image: Image.Image, path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    image.save(path, format="PNG", optimize=True, compress_level=9)


def generate_app_icon() -> list[Path]:
    if not ICON_MASTER.is_file():
        raise SystemExit(f"{MSG_NO_ICON_MASTER} {ICON_MASTER}")

    source = Image.open(ICON_MASTER).convert("RGBA")
    written: list[Path] = []
    for size in APPICON_SIZES:
        out = APPICON_DIR / f"icon_{size}.png"
        write_png(source.resize((size, size), Image.Resampling.LANCZOS), out)
        written.append(out)
    return written


def main() -> int:
    for out in generate_app_icon():
        print(f"{MSG_WROTE} {out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
