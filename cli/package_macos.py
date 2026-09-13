#!/usr/bin/env python3

from __future__ import annotations

"""Release-build Miw.app, ad-hoc sign it, and wrap it in a .dmg."""

import plistlib
import shutil
import sys
from pathlib import Path

from _helpers import run
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
    DIST,
    DIST_STAGING,
    DMG_FORMAT,
    DMG_SUFFIX,
    ENV_MACOSX_DEPLOYMENT_TARGET,
    ICNS_NAME,
    ICONSET_DIR_NAME,
    MACOS_MIN_VERSION,
    MSG_PACKAGE_MACOS_ONLY,
    MSG_SIGNED_ADHOC,
    SIGN_IDENTITY_ADHOC,
    SIGN_OPTIONS_RUNTIME,
)
from generate_icon import write_iconset
from package_common import (
    announce,
    build_release_binary,
    copy_resources,
    prepare_dist,
    resolve_version,
    write_checksum,
    write_github_output,
)


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


def write_app_icns(resources: Path) -> None:
    iconset = DIST / ICONSET_DIR_NAME
    write_iconset(iconset)
    icns = resources / ICNS_NAME
    run([BIN_ICONUTIL, "-c", "icns", "-o", str(icns), str(iconset)])
    shutil.rmtree(iconset)


def assemble_app(binary: Path, version: str) -> Path:
    prepare_dist()
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


def package_macos(raw_version: str | None = None) -> Path:
    if sys.platform != "darwin":
        raise SystemExit(MSG_PACKAGE_MACOS_ONLY)
    version = resolve_version(raw_version)
    binary = build_release_binary(env={ENV_MACOSX_DEPLOYMENT_TARGET: MACOS_MIN_VERSION})
    app = assemble_app(binary, version)
    sign_adhoc(app)
    dmg = make_dmg(app, version)
    write_checksum(dmg)
    write_github_output(version)
    return announce(dmg)


def main() -> int:
    package_macos(sys.argv[1] if len(sys.argv) > 1 else None)
    return 0


if __name__ == "__main__":
    sys.exit(main())
