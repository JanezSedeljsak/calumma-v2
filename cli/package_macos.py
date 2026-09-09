#!/usr/bin/env python3

from __future__ import annotations

"""Release-build Miw.app, ad-hoc sign it, and wrap it in a .dmg."""

import hashlib
import os
import plistlib
import shutil
import sys
from pathlib import Path

from _helpers import ENGINE_TARGET, require_matching_versions, run, workspace_version
from constants import (
    APP_BIN,
    APP_BUNDLE,
    APP_NAME,
    APPLICATIONS_LINK,
    APPLICATIONS_TARGET,
    BIN_CODESIGN,
    BIN_DITTO,
    BIN_HDIUTIL,
    BIN_ICONUTIL,
    BUNDLE_ID,
    CHECKSUM_SUFFIX,
    DESIGN,
    DIR_TARGET,
    DIST,
    DIST_STAGING,
    DMG_FORMAT,
    DMG_SUFFIX,
    ENCODING_UTF8,
    ENV_GITHUB_OUTPUT,
    ENV_MACOSX_DEPLOYMENT_TARGET,
    GUI,
    GUI_MANIFEST,
    ICNS_NAME,
    ICONSET_DIR_NAME,
    MACOS_MIN_VERSION,
    MSG_NO_APP,
    MSG_PACKAGE_MACOS_ONLY,
    MSG_PACKAGED,
    MSG_SIGNED_ADHOC,
    OUTPUT_KEY_DMG,
    OUTPUT_KEY_VERSION,
    SIGN_IDENTITY_ADHOC,
    SIGN_OPTIONS_RUNTIME,
    TOKENS_PATH,
    TRANSLATIONS,
)
from generate_icon import write_iconset

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


def build_release_binary() -> Path:
    run(
        ["cargo", "build", "--release", "--manifest-path", str(GUI_MANIFEST)],
        env={ENV_MACOSX_DEPLOYMENT_TARGET: MACOS_MIN_VERSION},
    )
    candidates = (
        ENGINE_TARGET / "release" / APP_BIN,
        GUI / DIR_TARGET / "release" / APP_BIN,
    )
    for path in candidates:
        if path.is_file():
            return path
    listed = " or ".join(str(path) for path in candidates)
    raise SystemExit(f"{MSG_NO_APP} {listed}")


def write_info_plist(dest: Path, version: str) -> None:
    payload = {
        "CFBundleDevelopmentRegion": "en",
        "CFBundleDisplayName": APP_NAME,
        "CFBundleExecutable": APP_BIN,
        "CFBundleIconFile": "AppIcon",
        "CFBundleIdentifier": BUNDLE_ID,
        "CFBundleInfoDictionaryVersion": "6.0",
        "CFBundleName": APP_NAME,
        "CFBundlePackageType": "APPL",
        "CFBundleShortVersionString": version,
        "CFBundleVersion": version,
        "LSMinimumSystemVersion": MACOS_MIN_VERSION,
        "NSHighResolutionCapable": True,
        "NSPrincipalClass": "NSApplication",
    }
    with dest.open("wb") as handle:
        plistlib.dump(payload, handle)


def copy_resources(resources: Path) -> None:
    for src, rel in RESOURCE_TREES:
        run([BIN_DITTO, str(src), str(resources / rel)])
    for src, rel in RESOURCE_FILES:
        dest = resources / rel
        dest.parent.mkdir(parents=True, exist_ok=True)
        run([BIN_DITTO, str(src), str(dest)])


def write_app_icns(resources: Path) -> None:
    iconset = DIST / ICONSET_DIR_NAME
    write_iconset(iconset)
    icns = resources / ICNS_NAME
    run([BIN_ICONUTIL, "-c", "icns", "-o", str(icns), str(iconset)])
    shutil.rmtree(iconset)


def assemble_app(binary: Path, version: str) -> Path:
    DIST.mkdir(parents=True, exist_ok=True)
    app = DIST / APP_BUNDLE
    if app.exists():
        shutil.rmtree(app)
    macos = app / "Contents" / "MacOS"
    resources = app / "Contents" / "Resources"
    macos.mkdir(parents=True)
    resources.mkdir(parents=True)
    run([BIN_DITTO, str(binary), str(macos / APP_BIN)])
    write_info_plist(app / "Contents" / "Info.plist", version)
    copy_resources(resources)
    write_app_icns(resources)
    return app


def sign_adhoc(app: Path) -> None:
    run(
        [
            BIN_CODESIGN,
            "--force",
            "--sign",
            SIGN_IDENTITY_ADHOC,
            "--options",
            SIGN_OPTIONS_RUNTIME,
            "--timestamp=none",
            str(app),
        ]
    )
    run([BIN_CODESIGN, "--verify", "--strict", str(app)])
    print(MSG_SIGNED_ADHOC)


def stage_dmg_root(app: Path) -> Path:
    if DIST_STAGING.exists():
        shutil.rmtree(DIST_STAGING)
    DIST_STAGING.mkdir(parents=True)
    run([BIN_DITTO, str(app), str(DIST_STAGING / app.name)])
    (DIST_STAGING / APPLICATIONS_LINK).symlink_to(APPLICATIONS_TARGET)
    return DIST_STAGING


def make_dmg(app: Path, version: str) -> Path:
    DIST.mkdir(parents=True, exist_ok=True)
    staging = stage_dmg_root(app)
    dmg = DIST / f"{APP_NAME}-{version}{DMG_SUFFIX}"
    dmg.unlink(missing_ok=True)
    run(
        [
            BIN_HDIUTIL,
            "create",
            "-volname",
            f"{APP_NAME} {version}",
            "-srcfolder",
            str(staging),
            "-ov",
            "-format",
            DMG_FORMAT,
            str(dmg),
        ]
    )
    shutil.rmtree(staging)
    return dmg


def write_checksum(dmg: Path) -> Path:
    digest = hashlib.sha256(dmg.read_bytes()).hexdigest()
    checksum = dmg.with_name(dmg.name + CHECKSUM_SUFFIX)
    checksum.write_text(f"{digest}  {dmg.name}\n", encoding=ENCODING_UTF8)
    return checksum


def write_github_output(dmg: Path, version: str) -> None:
    output_path = os.environ.get(ENV_GITHUB_OUTPUT)
    if not output_path:
        return
    with open(output_path, "a", encoding=ENCODING_UTF8) as f:
        f.write(f"{OUTPUT_KEY_DMG}={dmg}\n")
        f.write(f"{OUTPUT_KEY_VERSION}={version}\n")


def package_macos(raw_version: str | None = None) -> Path:
    if sys.platform != "darwin":
        raise SystemExit(MSG_PACKAGE_MACOS_ONLY)
    require_matching_versions()
    version = resolve_version(raw_version)
    binary = build_release_binary()
    app = assemble_app(binary, version)
    sign_adhoc(app)
    dmg = make_dmg(app, version)
    write_checksum(dmg)
    write_github_output(dmg, version)
    print(f"{MSG_PACKAGED} {dmg}")
    return dmg


def main() -> int:
    package_macos(sys.argv[1] if len(sys.argv) > 1 else None)
    return 0


if __name__ == "__main__":
    sys.exit(main())
