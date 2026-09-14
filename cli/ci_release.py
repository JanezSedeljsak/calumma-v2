#!/usr/bin/env python3

from __future__ import annotations

"""CI: resolve the release version and publish packaged artifacts as a GitHub release."""

import os
import sys
from pathlib import Path

from _helpers import run
from constants import (
    APP_NAME,
    BIN_GH,
    CHECKSUM_SUFFIX,
    DIST,
    ENCODING_UTF8,
    ENV_AUTO_RELEASE,
    ENV_GITHUB_REF_NAME,
    ENV_GITHUB_REF_TYPE,
    ENV_GITHUB_REPOSITORY,
    ENV_GITHUB_SHA,
    ENV_RELEASE_VERSION,
    ENV_RUNNER_TEMP,
    ENV_VERSION_INPUT,
    FILE_NOTES_MD,
    REF_TYPE_TAG,
)

INSTALLER_SUFFIXES = (
    ".dmg",
    ".zip",
    ".tar.gz",
    ".deb",
    ".rpm",
    ".AppImage",
    ".msi",
    ".exe",
)


def is_tag_push() -> bool:
    return os.environ.get(ENV_GITHUB_REF_TYPE) == REF_TYPE_TAG


def is_auto_release() -> bool:
    return os.environ.get(ENV_AUTO_RELEASE) == "true"


def resolve_ci_version() -> str:
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
    return not (is_tag_push() or is_auto_release())


def _is_installer(name: str) -> bool:
    return any(name.endswith(suffix) for suffix in INSTALLER_SUFFIXES)


def release_assets() -> list[Path]:
    if not DIST.is_dir():
        raise SystemExit(f"no release assets: {DIST} is missing")
    files = sorted(path for path in DIST.iterdir() if path.is_file())
    installers = [path for path in files if _is_installer(path.name)]
    checksums = [
        path
        for path in files
        if path.name.endswith(CHECKSUM_SUFFIX) and _is_installer(path.name[: -len(CHECKSUM_SUFFIX)])
    ]
    assets = installers + checksums
    if not installers:
        raise SystemExit(f"no installers in {DIST}")
    return assets


def _name_ending(assets: list[Path], suffix: str) -> str | None:
    for path in assets:
        if path.name.endswith(suffix):
            return path.name
    return None


def release_notes(version: str, assets: list[Path]) -> str:
    dmg = _name_ending(assets, ".dmg")
    windows = _name_ending(assets, ".msi") or _name_ending(assets, ".zip")
    deb = _name_ending(assets, ".deb")
    tarball = _name_ending(assets, ".tar.gz")
    lines = [f"## {APP_NAME} {version}", ""]
    if dmg:
        lines += [
            "### macOS",
            "",
            f"Download `{dmg}`, open it, and drag **{APP_NAME}** into **Applications**.",
            "",
            "This build is **ad-hoc signed, not notarized** — macOS Gatekeeper will refuse the",
            "first launch. Right-click the app → **Open** → **Open**, or run:",
            "",
            "```",
            f"xattr -dr com.apple.quarantine /Applications/{APP_NAME}.app",
            "```",
            "",
            "Apple Silicon only. Requires macOS 26 or later.",
            "",
        ]
    if windows:
        if windows.endswith(".msi"):
            windows_how = f"Download `{windows}` and run the installer."
        else:
            windows_how = f"Download `{windows}`, unzip, and run **{APP_NAME}.exe**."
        lines += [
            "### Windows",
            "",
            f"{windows_how} Windows 11, 64-bit.",
            "",
        ]
    if deb or tarball:
        lines += ["### Linux", ""]
        if deb:
            lines += [
                f"Debian/Ubuntu: `sudo apt install ./{deb}`",
                "",
            ]
        if tarball:
            lines += [
                f"Other distros: unpack `{tarball}` and run **{APP_NAME}**.",
                "",
            ]
        lines += [
            "Needs Vulkan and X11 (or XWayland).",
            "",
        ]
    lines += [
        "Verify a download:",
        "",
        "```",
        f"shasum -a 256 -c <artifact>{CHECKSUM_SUFFIX}",
        "```",
        "",
    ]
    return "\n".join(lines)


def release_exists(tag: str) -> bool:
    result = run([BIN_GH, "release", "view", tag], check=False, capture=True)
    return result.returncode == 0


def _repo() -> str:
    return os.environ[ENV_GITHUB_REPOSITORY]


def tag_ref_exists(tag: str) -> bool:
    result = run(
        [BIN_GH, "api", f"repos/{_repo()}/git/ref/tags/{tag}"],
        check=False,
        capture=True,
    )
    return result.returncode == 0


def ensure_tag_ref(tag: str, sha: str) -> None:
    if tag_ref_exists(tag):
        return
    run(
        [
            BIN_GH,
            "api",
            "--method",
            "POST",
            f"repos/{_repo()}/git/refs",
            "-f",
            f"ref=refs/tags/{tag}",
            "-f",
            f"sha={sha}",
        ]
    )


def ci_publish() -> None:
    version = os.environ[ENV_RELEASE_VERSION]
    assets = release_assets()
    tag = release_tag(version)
    notes_dir = Path(os.environ.get(ENV_RUNNER_TEMP, "/tmp"))
    notes_path = notes_dir / FILE_NOTES_MD
    notes_path.write_text(release_notes(version, assets), encoding=ENCODING_UTF8)
    paths = [str(path) for path in assets]

    if release_exists(tag):
        run([BIN_GH, "release", "upload", tag, *paths, "--clobber"])
        return

    ensure_tag_ref(tag, os.environ[ENV_GITHUB_SHA])
    cmd = [
        BIN_GH,
        "release",
        "create",
        tag,
        *paths,
        "--title",
        f"{APP_NAME} {version}",
        "--notes-file",
        str(notes_path),
    ]
    if is_prerelease():
        cmd.append("--prerelease")
    run(cmd)


def main() -> int:
    ci_publish()
    return 0


if __name__ == "__main__":
    sys.exit(main())
