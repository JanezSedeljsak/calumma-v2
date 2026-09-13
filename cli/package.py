#!/usr/bin/env python3

from __future__ import annotations

"""Dispatch `./manage.py package` to the host OS packager."""

import sys
from pathlib import Path

from constants import MSG_PACKAGE_UNSUPPORTED
from package_linux import package_linux
from package_macos import package_macos
from package_windows import package_windows


def package_host(raw_version: str | None = None) -> list[Path]:
    if sys.platform == "darwin":
        return [package_macos(raw_version)]
    if sys.platform == "win32":
        return [package_windows(raw_version)]
    if sys.platform.startswith("linux"):
        return package_linux(raw_version)
    raise SystemExit(MSG_PACKAGE_UNSUPPORTED)
