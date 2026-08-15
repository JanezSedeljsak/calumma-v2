#!/usr/bin/env python3

from __future__ import annotations

"""CI: detect whether engine/Cargo.toml's workspace version was bumped by this push."""

import os
import sys
import tomllib

from _helpers import run, workspace_version
from constants import (
    BIN_GIT,
    ENCODING_UTF8,
    ENV_GITHUB_OUTPUT,
    MSG_VERSION_BUMPED,
    MSG_VERSION_UNCHANGED,
)


def previous_version() -> str | None:
    result = run(
        [BIN_GIT, "show", "HEAD^:engine/Cargo.toml"],
        check=False,
        capture=True,
    )
    if result.returncode != 0:
        return None
    try:
        manifest = tomllib.loads(result.stdout)
        return str(manifest["workspace"]["package"]["version"])
    except KeyError, tomllib.TOMLDecodeError:
        return None


def write_output(changed: bool, version: str) -> None:
    lines = [f"changed={'true' if changed else 'false'}"]
    if changed:
        lines.append(f"version={version}")
    output_path = os.environ.get(ENV_GITHUB_OUTPUT)
    if not output_path:
        return
    with open(output_path, "a", encoding=ENCODING_UTF8) as f:
        for line in lines:
            f.write(line + "\n")


def check_version_bump() -> bool:
    new = workspace_version()
    old = previous_version()
    changed = new != old
    print(f"{MSG_VERSION_BUMPED if changed else MSG_VERSION_UNCHANGED} {new}", file=sys.stderr)
    write_output(changed, new)
    return changed


def main() -> int:
    check_version_bump()
    return 0


if __name__ == "__main__":
    sys.exit(main())
