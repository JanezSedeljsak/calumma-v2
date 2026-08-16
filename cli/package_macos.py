#!/usr/bin/env python3

from __future__ import annotations

"""Release-build Calumma.app, ad-hoc sign it, and wrap it in a .dmg."""

import hashlib
import os
import platform
import shutil
import sys
from pathlib import Path

from _helpers import MACOS, XCODE_PROJECT, run, workspace_version
from constants import (
    APP_BUNDLE,
    APP_NAME,
    APPLICATIONS_LINK,
    APPLICATIONS_TARGET,
    BIN_CODESIGN,
    BIN_DITTO,
    BIN_HDIUTIL,
    BIN_XCODEBUILD,
    CHECKSUM_SUFFIX,
    CONFIG_RELEASE,
    DIST,
    DIST_STAGING,
    DMG_FORMAT,
    DMG_SUFFIX,
    ENCODING_UTF8,
    ENV_CALUMMA_VERSION_OVERRIDE,
    ENV_GITHUB_OUTPUT,
    MACOS_DERIVED,
    MACOS_RELEASE_PRODUCTS,
    MSG_NO_APP,
    MSG_PACKAGED,
    MSG_SIGNED_ADHOC,
    OUTPUT_KEY_DMG,
    OUTPUT_KEY_VERSION,
    SCHEME_CALUMMA,
    SIGN_IDENTITY_ADHOC,
    SIGN_OPTIONS_RUNTIME,
)


def resolve_version(raw: str | None) -> str:
    cleaned = (raw or "").strip().removeprefix("refs/tags/").lstrip("vV").strip()
    return cleaned or workspace_version()


def host_arch() -> str:
    return platform.machine()


def build_release_app(version: str) -> Path:
    run(
        [
            BIN_XCODEBUILD,
            "-project",
            str(XCODE_PROJECT),
            "-scheme",
            SCHEME_CALUMMA,
            "-configuration",
            CONFIG_RELEASE,
            "-derivedDataPath",
            str(MACOS_DERIVED),
            f"MARKETING_VERSION={version}",
            f"CURRENT_PROJECT_VERSION={version}",
            f"ARCHS={host_arch()}",
            "ONLY_ACTIVE_ARCH=NO",
            "CODE_SIGN_STYLE=Manual",
            f"CODE_SIGN_IDENTITY={SIGN_IDENTITY_ADHOC}",
            "CODE_SIGNING_REQUIRED=YES",
            "CODE_SIGNING_ALLOWED=YES",
            "build",
        ],
        cwd=MACOS,
        # The "Stamp version from Cargo.toml" build phase self-heals MARKETING_VERSION on
        # every build so local Xcode/xcodebuild runs never ship a stale baked-in version —
        # but a release build's version can come from a git tag rather than Cargo.toml
        # (see resolve_version()), so it has to override that self-heal instead of losing
        # to it.
        env={ENV_CALUMMA_VERSION_OVERRIDE: version},
    )
    app = MACOS_RELEASE_PRODUCTS / APP_BUNDLE
    if not app.is_dir():
        raise SystemExit(f"{MSG_NO_APP} {app}")
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
    version = resolve_version(raw_version)
    app = build_release_app(version)
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
