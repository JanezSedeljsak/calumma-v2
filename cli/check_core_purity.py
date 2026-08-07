#!/usr/bin/env python3

from __future__ import annotations

import sys

from _helpers import FORBIDDEN_CORE_DEPS, ENGINE_MANIFEST, PKG_CORE, run
from constants import MSG_CORE_CLEAN, MSG_CORE_DIRTY, MSG_CORE_SKIP


def check_core_purity() -> int:
    meta = run(
        [
            "cargo",
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--manifest-path",
            str(ENGINE_MANIFEST),
        ],
        check=False,
        capture=True,
    )
    marker = f'"name":"{PKG_CORE}"'
    if meta.returncode != 0 or marker not in meta.stdout:
        print(MSG_CORE_SKIP)
        return 0

    tree = run(
        [
            "cargo",
            "tree",
            "-p",
            PKG_CORE,
            "-e",
            "normal,build",
            "--manifest-path",
            str(ENGINE_MANIFEST),
        ],
        capture=True,
    ).stdout

    hits = [line for line in tree.splitlines() if FORBIDDEN_CORE_DEPS.search(line)]
    if hits:
        print(MSG_CORE_DIRTY)
        print("\n".join(hits))
        return 1

    print(MSG_CORE_CLEAN)
    return 0


def main() -> int:
    return check_core_purity()


if __name__ == "__main__":
    sys.exit(main())
