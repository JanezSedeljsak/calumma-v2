#!/usr/bin/env python3

from __future__ import annotations

"""Shared version, cargo build, resource copy, and checksum helpers for packagers."""

import hashlib
import os
import shutil
import sys
from pathlib import Path

from _helpers import ENGINE_TARGET, require_matching_versions, run, workspace_version
from constants import (
    APP_BIN,
    CHECKSUM_SUFFIX,
    DESIGN,
    DIR_TARGET,
    DIST,
    ENCODING_UTF8,
    ENV_GITHUB_OUTPUT,
    GUI,
    GUI_MANIFEST,
    MSG_NO_APP,
    MSG_PACKAGED,
    OUTPUT_KEY_VERSION,
    TOKENS_PATH,
    TRANSLATIONS,
)

RESOURCE_TREES = (
    (DESIGN / "icons", Path("design") / "icons"),
    (TRANSLATIONS, Path("translations")),
)
RESOURCE_FILES = (
    (DESIGN / "icon.png", Path("design") / "icon.png"),
    (TOKENS_PATH, Path("design") / "tokens.json"),
)


def resolve_version(raw: str | None) -> str:
    cleaned = (raw or "").strip().removeprefix("refs/tags/").lstrip("vV").strip()
    return cleaned or workspace_version()


def app_bin_name() -> str:
    suffix = ".exe" if sys.platform == "win32" else ""
    return f"{APP_BIN}{suffix}"


def find_release_binary() -> Path:
    name = app_bin_name()
    candidates = (
        ENGINE_TARGET / "release" / name,
        GUI / DIR_TARGET / "release" / name,
    )
    for path in candidates:
        if path.is_file():
            return path
    listed = " or ".join(str(path) for path in candidates)
    raise SystemExit(f"{MSG_NO_APP} {listed}")


def build_release_binary(*, env: dict[str, str] | None = None) -> Path:
    require_matching_versions()
    run(
        ["cargo", "build", "--release", "--manifest-path", str(GUI_MANIFEST)],
        env=env,
    )
    return find_release_binary()


def copy_resources(dest: Path) -> None:
    for src, rel in RESOURCE_TREES:
        target = dest / rel
        if target.exists():
            shutil.rmtree(target)
        shutil.copytree(src, target)
    for src, rel in RESOURCE_FILES:
        target = dest / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(src, target)


def write_checksum(path: Path) -> Path:
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    checksum = path.with_name(path.name + CHECKSUM_SUFFIX)
    checksum.write_text(f"{digest}  {path.name}\n", encoding=ENCODING_UTF8)
    return checksum


def write_github_output(version: str) -> None:
    output_path = os.environ.get(ENV_GITHUB_OUTPUT)
    if not output_path:
        return
    with open(output_path, "a", encoding=ENCODING_UTF8) as handle:
        handle.write(f"{OUTPUT_KEY_VERSION}={version}\n")


def announce(path: Path) -> Path:
    print(f"{MSG_PACKAGED} {path}")
    return path


def prepare_dist() -> Path:
    DIST.mkdir(parents=True, exist_ok=True)
    return DIST
