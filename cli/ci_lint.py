#!/usr/bin/env python3

from __future__ import annotations

"""CI-equivalent fmt / clippy / ruff checks — single source of truth for manage.py and GHA."""

import sys

from _helpers import cargo_cmd, python_module, require_matching_versions, run, which
from constants import (
    BIN_TAPLO,
    CLIPPY_DARWIN_PACKAGES,
    CLIPPY_PORTABLE_PACKAGES,
    ENGINE,
    FILE_CARGO_TOML,
    GUI_MANIFEST,
    PYTHON_PATHS,
)

CLIPPY_ARGS = ("--all-targets", "--", "-D", "warnings")


def _package_args(packages: tuple[str, ...]) -> list[str]:
    out: list[str] = []
    for package in packages:
        out.extend(["-p", package])
    return out


def run_rustfmt(manifest: str, *, check: bool) -> None:
    args = ["--manifest-path", manifest, "--all"]
    if check:
        args.extend(["--", "--check"])
    run(["cargo", "fmt", *args])


def run_ruff(*, check: bool) -> None:
    if check:
        run([*python_module("ruff", "format", "--check"), *PYTHON_PATHS])
        run([*python_module("ruff", "check"), *PYTHON_PATHS])
        return
    run([*python_module("ruff", "format"), *PYTHON_PATHS])
    run([*python_module("ruff", "check", "--fix"), *PYTHON_PATHS])


def run_taplo_fmt(*, check: bool) -> None:
    if not which(BIN_TAPLO):
        return
    tomls = [
        str(ENGINE / FILE_CARGO_TOML),
        *[str(p) for p in ENGINE.glob(f"*/{FILE_CARGO_TOML}")],
    ]
    args = [BIN_TAPLO, "fmt"]
    if check:
        args.append("--check")
    args.extend(tomls)
    run(args)


def run_fmt(*, check: bool, include_taplo: bool = True) -> None:
    run_rustfmt(str(ENGINE / FILE_CARGO_TOML), check=check)
    run_rustfmt(str(GUI_MANIFEST), check=check)
    run_ruff(check=check)
    if include_taplo:
        run_taplo_fmt(check=check)


def run_clippy_portable() -> None:
    run(cargo_cmd("clippy", *_package_args(CLIPPY_PORTABLE_PACKAGES), *CLIPPY_ARGS))


def run_clippy_darwin() -> None:
    if sys.platform != "darwin":
        return
    run(cargo_cmd("clippy", *_package_args(CLIPPY_DARWIN_PACKAGES), *CLIPPY_ARGS))


def run_clippy(*, darwin_only: bool = False) -> None:
    if darwin_only:
        run_clippy_darwin()
        return
    run_clippy_portable()
    run_clippy_darwin()


def run_ci_lint() -> None:
    require_matching_versions()
    run_fmt(check=True, include_taplo=False)
    run_clippy_portable()


def run_ci_test() -> None:
    if sys.platform == "linux":
        run(cargo_cmd("test", "-p", "calumma-core", "--all-targets"))
        run(
            cargo_cmd("test", "-p", "calumma-render", "--all-targets"),
            env={"WGPU_BACKEND": "vulkan"},
        )
        return
    run(cargo_cmd("test", "--workspace"))
