#!/usr/bin/env python3

from __future__ import annotations

"""CI: resolve the release version and publish the packaged .dmg as a GitHub release."""

import os
import sys
from pathlib import Path

from _helpers import run
from constants import (
    APP_NAME,
    BIN_GH,
    CHECKSUM_SUFFIX,
    ENCODING_UTF8,
    ENV_AUTO_RELEASE,
    ENV_GITHUB_REF_NAME,
    ENV_GITHUB_REF_TYPE,
    ENV_GITHUB_SHA,
    ENV_RELEASE_DMG,
    ENV_RELEASE_VERSION,
    ENV_RUNNER_TEMP,
    ENV_VERSION_INPUT,
    FILE_NOTES_MD,
    REF_TYPE_TAG,
)


def is_tag_push() -> bool:
    return os.environ.get(ENV_GITHUB_REF_TYPE) == REF_TYPE_TAG


def is_auto_release() -> bool:
    return os.environ.get(ENV_AUTO_RELEASE) == "true"


def resolve_ci_version() -> str:
    """The version to stamp: an explicit workflow_dispatch input, else the pushed tag name,
    else empty (package_macos falls back to the engine/Cargo.toml workspace version)."""
    version_input = (os.environ.get(ENV_VERSION_INPUT) or "").strip()
    if version_input:
        return version_input
    if is_tag_push():
        return os.environ.get(ENV_GITHUB_REF_NAME, "")
    return ""


def release_tag(version: str) -> str:
    if is_tag_push():
        return os.environ.get(ENV_GITHUB_REF_NAME, f"v{version}")
    return f"v{version}"


def is_prerelease() -> bool:
    """A tag push is an intentional release; a version bump landing on main auto-publishes
    for real too. Everything else (plain workflow_dispatch) stays a prerelease."""
    return not (is_tag_push() or is_auto_release())


def release_notes(version: str, dmg_name: str) -> str:
    return (
        f"## {APP_NAME} {version} — macOS\n\n"
        f"Download `{dmg_name}`, open it, and drag **{APP_NAME}** into **Applications**.\n\n"
        "This build is **ad-hoc signed, not notarized** — macOS Gatekeeper will refuse the\n"
        "first launch. Right-click the app → **Open** → **Open**, or run:\n\n"
        "```\n"
        f"xattr -dr com.apple.quarantine /Applications/{APP_NAME}.app\n"
        "```\n\n"
        "Apple Silicon only. Requires macOS 26 or later.\n\n"
        "Verify the download:\n\n"
        "```\n"
        f"shasum -a 256 -c {dmg_name}.sha256\n"
        "```\n"
    )


def release_exists(tag: str) -> bool:
    result = run([BIN_GH, "release", "view", tag], check=False, capture=True)
    return result.returncode == 0


def ci_publish() -> None:
    version = os.environ[ENV_RELEASE_VERSION]
    dmg = Path(os.environ[ENV_RELEASE_DMG])
    checksum = dmg.with_name(dmg.name + CHECKSUM_SUFFIX)
    tag = release_tag(version)

    notes_dir = Path(os.environ.get(ENV_RUNNER_TEMP, "/tmp"))
    notes_path = notes_dir / FILE_NOTES_MD
    notes_path.write_text(release_notes(version, dmg.name), encoding=ENCODING_UTF8)

    if release_exists(tag):
        run([BIN_GH, "release", "upload", tag, str(dmg), str(checksum), "--clobber"])
        return

    cmd = [
        BIN_GH,
        "release",
        "create",
        tag,
        str(dmg),
        str(checksum),
        "--title",
        f"{APP_NAME} {version}",
        "--notes-file",
        str(notes_path),
        "--target",
        os.environ[ENV_GITHUB_SHA],
    ]
    if is_prerelease():
        cmd.append("--prerelease")
    run(cmd)


def main() -> int:
    ci_publish()
    return 0


if __name__ == "__main__":
    sys.exit(main())
