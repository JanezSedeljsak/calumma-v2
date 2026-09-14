#!/usr/bin/env python3

from __future__ import annotations

"""Release-build Miw.exe and wrap it with design/translations in an MSI."""

import os
import shutil
import sys
import uuid
from pathlib import Path
from xml.sax.saxutils import escape

from _helpers import run, which
from constants import (
    APP_BIN,
    APP_NAME,
    BIN_DOTNET,
    BIN_WIX,
    BUNDLE_ID,
    DIST,
    MSG_NO_WIX,
    MSG_PACKAGE_WINDOWS_ONLY,
    MSI_SUFFIX,
    REPO_HOMEPAGE,
    WINDOWS_ARCH,
    WIX_VERSION,
)
from generate_icon import write_ico
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

MSI_STAGING = DIST / "msi-root"
PAYLOAD = MSI_STAGING / "payload"
WXS_NAME = "Product.wxs"
ICO_NAME = "AppIcon.ico"
ICON_ID = "AppIcon.exe"
MENU_FOLDER_ID = "AppMenuFolder"
INSTALL_FOLDER_ID = "INSTALLFOLDER"


def _attr(value: str) -> str:
    return escape(value, {'"': "&quot;"})


def upgrade_code() -> str:
    return "{" + str(uuid.uuid5(uuid.NAMESPACE_DNS, BUNDLE_ID)).upper() + "}"


def _dotnet_tools() -> Path:
    return Path.home() / ".dotnet" / "tools"


def _prepend_dotnet_tools() -> None:
    tools = str(_dotnet_tools())
    current = os.environ.get("PATH", "")
    if current.split(os.pathsep)[0] != tools:
        os.environ["PATH"] = tools + os.pathsep + current


def ensure_wix() -> str:
    _prepend_dotnet_tools()
    found = which(BIN_WIX)
    if found:
        return found
    if not which(BIN_DOTNET):
        raise SystemExit(MSG_NO_WIX)
    run(
        [BIN_DOTNET, "tool", "install", "--global", "wix", "--version", WIX_VERSION],
        check=False,
    )
    _prepend_dotnet_tools()
    found = which(BIN_WIX)
    if found:
        return found
    raise SystemExit(MSG_NO_WIX)


def write_wxs(dest: Path, payload: Path, ico: Path, version: str) -> Path:
    include = str(payload.resolve() / "**")
    ico_source = str(ico.resolve())
    dest.write_text(
        (
            '<?xml version="1.0" encoding="UTF-8"?>\n'
            '<Wix xmlns="http://wixtoolset.org/schemas/v4/wxs">\n'
            f'  <Package Name="{_attr(APP_NAME)}" Manufacturer="{_attr(APP_NAME)}" '
            f'Version="{_attr(version)}" UpgradeCode="{upgrade_code()}" '
            'Language="1033" Scope="perMachine">\n'
            f'    <SummaryInformation Description="{_attr(APP_NAME)} — your personal whiteboard" />\n'
            "    <MajorUpgrade "
            'DowngradeErrorMessage="A newer version of [ProductName] is already installed." />\n'
            '    <MediaTemplate EmbedCab="yes" CompressionLevel="high" />\n'
            f'    <Icon Id="{ICON_ID}" SourceFile="{_attr(ico_source)}" />\n'
            f'    <Property Id="ARPPRODUCTICON" Value="{ICON_ID}" />\n'
            f'    <Property Id="ARPHELPLINK" Value="{_attr(REPO_HOMEPAGE)}" />\n'
            f'    <Property Id="ARPURLINFOABOUT" Value="{_attr(REPO_HOMEPAGE)}" />\n'
            '    <Property Id="ARPNOMODIFY" Value="1" />\n'
            '    <StandardDirectory Id="ProgramFiles64Folder">\n'
            f'      <Directory Id="{INSTALL_FOLDER_ID}" Name="{_attr(APP_NAME)}" />\n'
            "    </StandardDirectory>\n"
            '    <StandardDirectory Id="ProgramMenuFolder">\n'
            f'      <Directory Id="{MENU_FOLDER_ID}" Name="{_attr(APP_NAME)}" />\n'
            "    </StandardDirectory>\n"
            f'    <Feature Id="Main" Title="{_attr(APP_NAME)}" Level="1">\n'
            f'      <Files Directory="{INSTALL_FOLDER_ID}" Include="{_attr(include)}" />\n'
            f'      <Component Id="StartMenuShortcut" Directory="{MENU_FOLDER_ID}">\n'
            f'        <Shortcut Name="{_attr(APP_NAME)}" '
            'Description="Your personal whiteboard" '
            f'Target="[{INSTALL_FOLDER_ID}]{app_bin_name()}" '
            f'WorkingDirectory="{INSTALL_FOLDER_ID}" Icon="{ICON_ID}" />\n'
            '        <RemoveFolder On="uninstall" />\n'
            f'        <RegistryValue Root="HKMU" Key="Software\\{_attr(APP_NAME)}" '
            'Name="installed" Type="integer" Value="1" KeyPath="yes" />\n'
            "      </Component>\n"
            "    </Feature>\n"
            "  </Package>\n"
            "</Wix>\n"
        ),
        encoding="utf-8",
    )
    return dest


def make_msi(binary: Path, version: str) -> Path:
    ensure_wix()
    if MSI_STAGING.exists():
        shutil.rmtree(MSI_STAGING)
    PAYLOAD.mkdir(parents=True)
    shutil.copy2(binary, PAYLOAD / app_bin_name())
    copy_resources(PAYLOAD)
    ico = write_ico(MSI_STAGING / ICO_NAME)
    wxs = write_wxs(MSI_STAGING / WXS_NAME, PAYLOAD, ico, version)
    msi = DIST / f"{APP_BIN}-{version}-windows-{WINDOWS_ARCH}{MSI_SUFFIX}"
    msi.unlink(missing_ok=True)
    run(
        [
            BIN_WIX,
            "build",
            "-arch",
            "x64",
            "-pdbType",
            "none",
            "-o",
            str(msi),
            str(wxs),
        ]
    )
    shutil.rmtree(MSI_STAGING)
    return msi


def package_windows(raw_version: str | None = None) -> Path:
    if sys.platform != "win32":
        raise SystemExit(MSG_PACKAGE_WINDOWS_ONLY)
    version = resolve_version(raw_version)
    binary = build_release_binary()
    prepare_dist()
    msi = make_msi(binary, version)
    write_checksum(msi)
    write_github_output(version)
    return announce(msi)


def main() -> int:
    package_windows(sys.argv[1] if len(sys.argv) > 1 else None)
    return 0


if __name__ == "__main__":
    sys.exit(main())
