#!/usr/bin/env python3

from __future__ import annotations

import sys
from pathlib import Path

from constants import DESIGN, MSG_WROTE, ROOT
from PIL import Image

EXAMPLE_DIR = DESIGN / "example"
SOURCE_DIR = EXAMPLE_DIR / "source"
README_MAX_WIDTH = 1400


def write_png(image: Image.Image, path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    image.save(path, format="PNG", optimize=True, compress_level=9)


def optimize_for_readme(image: Image.Image, max_width: int = README_MAX_WIDTH) -> Image.Image:
    out = image.convert("RGBA")
    if out.width > max_width:
        scale = max_width / out.width
        size = (max_width, max(1, round(out.height * scale)))
        out = out.resize(size, Image.Resampling.LANCZOS)
    return out


def prepare_screenshot(source: Path, dest: Path) -> None:
    write_png(optimize_for_readme(Image.open(source).convert("RGBA")), dest)


def main() -> int:
    EXAMPLE_DIR.mkdir(parents=True, exist_ok=True)
    SOURCE_DIR.mkdir(parents=True, exist_ok=True)

    editor_src = SOURCE_DIR / "editor.png"
    landing_src = SOURCE_DIR / "landing.png"
    editor_out = EXAMPLE_DIR / "editor.png"
    landing_out = EXAMPLE_DIR / "landing.png"

    screen = SOURCE_DIR / "screen.png"
    if not editor_src.is_file() and screen.is_file():
        editor_src = screen

    if not landing_src.is_file() and screen.is_file():
        landing_src = screen

    written: list[Path] = []

    if editor_src.is_file():
        prepare_screenshot(editor_src, editor_out)
        written.append(editor_out)

    if landing_src.is_file():
        prepare_screenshot(landing_src, landing_out)
        written.append(landing_out)

    if not written:
        print(
            "drop PNGs into design/example/source/ as editor.png and landing.png "
            "(or screen.png), then rerun ./manage.py examples",
            file=sys.stderr,
        )
        return 1

    for path in written:
        size_kb = path.stat().st_size // 1024
        with Image.open(path) as img:
            print(f"{MSG_WROTE} {path.relative_to(ROOT)} ({img.width}x{img.height}, {size_kb} KB)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
