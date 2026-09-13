#!/usr/bin/env python3

from __future__ import annotations

"""Release-build Miw and wrap it in a portable tarball plus a .deb."""

import shutil
import stat
import sys
import tarfile
from pathlib import Path

from _helpers import run, which
from constants import (
    APP_BIN,
    APP_NAME,
    BIN_DPKG_DEB,
    DEB_ARCH,
    DESIGN,
    DIST,
    ENCODING_UTF8,
    LINUX_ARCH,
    LINUX_BIN,
    LINUX_SHARE,
    MSG_NO_DPKG_DEB,
    MSG_PACKAGE_LINUX_ONLY,
    TAR_GZ_SUFFIX,
)
from package_common import (
    announce,
    build_release_binary,
    copy_resources,
    prepare_dist,
    resolve_version,
    write_checksum,
    write_github_output,
)

DEB_STAGING = DIST / "deb-root"
TARBALL_STAGING = DIST / "tar-root"


def _chmod_executable(path: Path) -> None:
    mode = path.stat().st_mode
    path.chmod(mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)


def _desktop_entry() -> str:
    return (
        "[Desktop Entry]\n"
        "Type=Application\n"
        f"Name={APP_NAME}\n"
        "Comment=Your personal whiteboard\n"
        f"Exec={LINUX_BIN}\n"
        f"Icon={LINUX_BIN}\n"
        "Terminal=false\n"
        "Categories=Graphics;2DGraphics;RasterGraphics;\n"
        f"StartupWMClass={APP_BIN}\n"
    )


def _control_file(version: str, installed_size_kb: int) -> str:
    return (
        f"Package: {LINUX_BIN}\n"
        f"Version: {version}\n"
        "Section: graphics\n"
        "Priority: optional\n"
        f"Architecture: {DEB_ARCH}\n"
        "Depends: libvulkan1, libx11-6, libxext6, libxfixes3, libxkbcommon0, libfontconfig1, "
        "libwayland-client0\n"
        "Maintainer: Miw <janezsedeljsak@users.noreply.github.com>\n"
        "Homepage: https://github.com/JanezSedeljsak/calumma-v2\n"
        f"Installed-Size: {installed_size_kb}\n"
        f"Description: {APP_NAME} — your personal whiteboard\n"
        " Bounded project canvases you draw on with a pen, shapes, and text.\n"
    )


def _tree_size_kb(root: Path) -> int:
    total = 0
    for path in root.rglob("*"):
        if path.is_file():
            total += path.stat().st_size
    return max(1, (total + 1023) // 1024)


def make_tarball(binary: Path, version: str) -> Path:
    if TARBALL_STAGING.exists():
        shutil.rmtree(TARBALL_STAGING)
    folder = TARBALL_STAGING / f"{APP_NAME}-{version}-linux-{LINUX_ARCH}"
    folder.mkdir(parents=True)
    shutil.copy2(binary, folder / APP_BIN)
    _chmod_executable(folder / APP_BIN)
    copy_resources(folder)
    tarball = DIST / f"{APP_NAME}-{version}-linux-{LINUX_ARCH}{TAR_GZ_SUFFIX}"
    tarball.unlink(missing_ok=True)
    with tarfile.open(tarball, "w:gz") as archive:
        archive.add(folder, arcname=folder.name)
    shutil.rmtree(TARBALL_STAGING)
    return tarball


def make_deb(binary: Path, version: str) -> Path:
    if not which(BIN_DPKG_DEB):
        raise SystemExit(MSG_NO_DPKG_DEB)
    if DEB_STAGING.exists():
        shutil.rmtree(DEB_STAGING)
    data = DEB_STAGING / "usr"
    bindir = data / "bin"
    share = data / "share" / LINUX_SHARE
    apps = data / "share" / "applications"
    icons = data / "share" / "icons" / "hicolor" / "256x256" / "apps"
    debian = DEB_STAGING / "DEBIAN"
    for path in (bindir, share, apps, icons, debian):
        path.mkdir(parents=True)
    shutil.copy2(binary, bindir / LINUX_BIN)
    _chmod_executable(bindir / LINUX_BIN)
    copy_resources(share)
    (apps / f"{LINUX_BIN}.desktop").write_text(_desktop_entry(), encoding=ENCODING_UTF8)
    icon_src = DESIGN / "icon-rounded.png"
    if not icon_src.is_file():
        icon_src = DESIGN / "icon.png"
    shutil.copy2(icon_src, icons / f"{LINUX_BIN}.png")
    installed = _tree_size_kb(data)
    control = debian / "control"
    control.write_text(_control_file(version, installed), encoding=ENCODING_UTF8)
    deb = DIST / f"{APP_NAME}-{version}-linux-{DEB_ARCH}.deb"
    deb.unlink(missing_ok=True)
    env = {"SOURCE_DATE_EPOCH": "0"}
    run(
        [BIN_DPKG_DEB, "--build", "--root-owner-group", str(DEB_STAGING), str(deb)],
        env=env,
    )
    shutil.rmtree(DEB_STAGING)
    return deb


def package_linux(raw_version: str | None = None) -> list[Path]:
    if not sys.platform.startswith("linux"):
        raise SystemExit(MSG_PACKAGE_LINUX_ONLY)
    version = resolve_version(raw_version)
    binary = build_release_binary()
    prepare_dist()
    artifacts = [make_tarball(binary, version), make_deb(binary, version)]
    for path in artifacts:
        write_checksum(path)
        announce(path)
    write_github_output(version)
    return artifacts


def main() -> int:
    package_linux(sys.argv[1] if len(sys.argv) > 1 else None)
    return 0


if __name__ == "__main__":
    sys.exit(main())
