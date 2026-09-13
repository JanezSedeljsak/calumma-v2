#!/usr/bin/env python3

from __future__ import annotations

"""Release-build Miw.exe and wrap it with design/translations in a portable zip."""

import shutil
import sys
import zipfile
from pathlib import Path

from constants import (
    APP_BIN,
    DIST,
    MSG_PACKAGE_WINDOWS_ONLY,
    WINDOWS_ARCH,
    ZIP_SUFFIX,
)
from package_common import (
    announce,
    app_bin_name,
    build_release_binary,
    copy_resources,
    prepare_dist,
    resolve_version,
    write_checksum,
    write_github_output,
)

RESOURCE_ROOT_NAME = "_resources"


def package_windows(raw_version: str | None = None) -> Path:
    if sys.platform != "win32":
        raise SystemExit(MSG_PACKAGE_WINDOWS_ONLY)
    version = resolve_version(raw_version)
    binary = build_release_binary()
    prepare_dist()
    staging = DIST / RESOURCE_ROOT_NAME
    if staging.exists():
        shutil.rmtree(staging)
    staging.mkdir(parents=True)
    copy_resources(staging)
    zip_path = DIST / f"{APP_BIN}-{version}-windows-{WINDOWS_ARCH}{ZIP_SUFFIX}"
    zip_path.unlink(missing_ok=True)
    with zipfile.ZipFile(zip_path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        archive.write(binary, app_bin_name())
        for path in staging.rglob("*"):
            if path.is_file():
                archive.write(path, path.relative_to(staging).as_posix())
    shutil.rmtree(staging)
    write_checksum(zip_path)
    write_github_output(version)
    return announce(zip_path)


def main() -> int:
    package_windows(sys.argv[1] if len(sys.argv) > 1 else None)
    return 0


if __name__ == "__main__":
    sys.exit(main())
