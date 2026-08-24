#!/usr/bin/env python3

from __future__ import annotations

"""Shared helpers for calumma-v2 Python tooling."""

import json
import os
import re
import shutil
import subprocess
import sys
import tomllib
from pathlib import Path

from constants import (
    BIN_CARGO,
    COLOR_KEYS,
    CRATE_DIRS,
    CRATE_PREFIX,
    DIR_ENGINE,
    ENCODING_UTF8,
    ENGINE,
    ENGINE_LOCK,
    ENGINE_MANIFEST,
    ENGINE_TARGET,
    ENV_CARGO_TARGET_DIR,
    ENV_GITHUB_STEP_SUMMARY,
    MACOS,
    MSG_N_A,
    PKG_CORE,
    ROOT,
    SWIFT_FORMAT_CONFIG,
    TOKEN_ACCENT_ORANGE,
    TOKEN_ACCENT_TEAL,
    TOKEN_KEY_ACCENT,
    TOKEN_KEY_COLOR,
    TOKEN_KEY_CONTROL,
    TOKEN_KEY_PRESETS,
    TOKEN_KEY_RADIUS,
    TOKEN_KEY_SPACE,
    TOKEN_KEY_TYPE,
    TOKEN_KEY_WINDOW,
    TOKEN_MODE_DARK,
    TOKEN_MODE_LIGHT,
    TOKENS_PATH,
    TOKENS_SWIFT_OUT,
    XCODE_PROJECT,
)

FORBIDDEN_CORE_DEPS = re.compile(r"wgpu|objc2|metal|windows", re.IGNORECASE)

__all__ = [
    "COLOR_KEYS",
    "ENGINE",
    "ENGINE_LOCK",
    "ENGINE_MANIFEST",
    "ENGINE_TARGET",
    "MACOS",
    "PKG_CORE",
    "ROOT",
    "SWIFT_FORMAT_CONFIG",
    "TOKENS_PATH",
    "TOKENS_SWIFT_OUT",
    "XCODE_PROJECT",
    "cargo_cmd",
    "crate_for_path",
    "ensure_engine_env",
    "format_coverage_markdown",
    "format_pct",
    "format_pct_with_count",
    "hex_to_srgb",
    "load_tokens",
    "print_coverage_table",
    "python_module",
    "run",
    "write_github_summary",
    "swift_color_lit",
    "token_colors",
    "token_presets",
    "token_radius",
    "token_space",
    "token_type",
    "token_window",
    "which",
    "workspace_version",
    "FORBIDDEN_CORE_DEPS",
]


def ensure_engine_env() -> None:
    os.environ.setdefault(ENV_CARGO_TARGET_DIR, str(ENGINE_TARGET))


def workspace_version() -> str:
    """Single source of truth for the app version: engine/Cargo.toml's [workspace.package]."""
    manifest = tomllib.loads(ENGINE_MANIFEST.read_text(encoding=ENCODING_UTF8))
    return str(manifest["workspace"]["package"]["version"])


def python_module(*args: str) -> list[str]:
    return [sys.executable, "-m", *args]


def cargo_cmd(subcommand: str, *args: str) -> list[str]:
    ensure_engine_env()
    return [BIN_CARGO, subcommand, "--manifest-path", str(ENGINE_MANIFEST), *args]


def run(
    cmd: list[str],
    *,
    cwd: Path | None = None,
    check: bool = True,
    capture: bool = False,
    env: dict[str, str] | None = None,
) -> subprocess.CompletedProcess[str]:
    merged = os.environ.copy()
    if env:
        merged.update(env)
    return subprocess.run(
        cmd,
        cwd=cwd or ROOT,
        check=check,
        capture_output=capture,
        text=True,
        env=merged,
    )


def which(name: str) -> str | None:
    return shutil.which(name)


def load_tokens() -> dict[str, object]:
    return json.loads(TOKENS_PATH.read_text(encoding=ENCODING_UTF8))


def _float_map(section: object) -> dict[str, float]:
    assert isinstance(section, dict)
    return {str(k): float(v) for k, v in section.items()}


def token_radius(tokens: dict[str, object] | None = None) -> dict[str, float]:
    return _float_map((tokens or load_tokens())[TOKEN_KEY_RADIUS])


def token_space(tokens: dict[str, object] | None = None) -> dict[str, float]:
    return _float_map((tokens or load_tokens())[TOKEN_KEY_SPACE])


def token_control(tokens: dict[str, object] | None = None) -> dict[str, float]:
    return _float_map((tokens or load_tokens())[TOKEN_KEY_CONTROL])


def token_type(tokens: dict[str, object] | None = None) -> dict[str, float]:
    return _float_map((tokens or load_tokens())[TOKEN_KEY_TYPE])


def token_window(tokens: dict[str, object] | None = None) -> dict[str, float]:
    return _float_map((tokens or load_tokens())[TOKEN_KEY_WINDOW])


def token_presets(tokens: dict[str, object] | None = None) -> list[dict[str, object]]:
    raw = (tokens or load_tokens())[TOKEN_KEY_PRESETS]
    assert isinstance(raw, list)
    return [p for p in raw if isinstance(p, dict)]


def token_colors(tokens: dict[str, object] | None = None) -> dict[str, dict[str, object]]:
    color = (tokens or load_tokens())[TOKEN_KEY_COLOR]
    assert isinstance(color, dict)
    out: dict[str, dict[str, object]] = {}
    for mode in (TOKEN_MODE_LIGHT, TOKEN_MODE_DARK):
        mode_data = color[mode]
        assert isinstance(mode_data, dict)
        out[mode] = mode_data
    return out


def hex_to_srgb(h: str) -> tuple[float, float, float, float]:
    h = h.lstrip("#")
    if len(h) == 6:
        r, g, b = int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16)
        return r / 255, g / 255, b / 255, 1.0
    if len(h) == 8:
        r, g, b, a = int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16), int(h[6:8], 16)
        return r / 255, g / 255, b / 255, a / 255
    raise ValueError(h)


def swift_color_lit(name: str, h: str) -> str:
    r, g, b, a = hex_to_srgb(h)
    return f"    static let {name} = Color(red: {r:.6f}, green: {g:.6f}, blue: {b:.6f}, opacity: {a:.6f})"


def crate_for_path(path: str) -> str | None:
    parts = Path(path).parts
    try:
        i = parts.index(DIR_ENGINE)
    except ValueError:
        return None
    if i + 1 >= len(parts):
        return None
    crate_dir = parts[i + 1]
    if crate_dir in CRATE_DIRS:
        return f"{CRATE_PREFIX}{crate_dir}"
    return None


def format_pct(covered: int, total: int) -> str:
    if total <= 0:
        return MSG_N_A
    return f"{(100.0 * covered / total):5.1f}%"


def format_pct_with_count(covered: int, total: int) -> str:
    pct = format_pct(covered, total)
    if total <= 0:
        return pct
    return f"{pct} ({covered}/{total})"


def print_coverage_table(rows: list[dict[str, object]]) -> None:
    name_w = max(len(str(r["crate"])) for r in rows)
    name_w = max(name_w, 5)
    lines_w = max(len(str(r["lines"])) for r in rows)
    lines_w = max(lines_w, 5)
    print()
    print(f"{'crate':<{name_w}}  {'lines':>{lines_w}}  {'funcs':>7}  {'regs':>7}")
    print(f"{'-' * name_w}  {'-' * lines_w}  {'------':>7}  {'------':>7}")
    for row in rows:
        print(
            f"{str(row['crate']):<{name_w}}  "
            f"{str(row['lines']):>{lines_w}}  "
            f"{str(row['funcs']):>7}  "
            f"{str(row['regions']):>7}"
        )
    print()


def format_coverage_markdown(rows: list[dict[str, object]], title: str) -> str:
    lines = [
        f"### {title}",
        "",
        "| crate | lines (cov/total) | funcs | regions |",
        "| --- | ---: | ---: | ---: |",
    ]
    for row in rows:
        lines.append(f"| {row['crate']} | {row['lines']} | {row['funcs']} | {row['regions']} |")
    lines.append("")
    return "\n".join(lines)


def write_github_summary(markdown: str) -> None:
    summary_path = os.environ.get(ENV_GITHUB_STEP_SUMMARY)
    if not summary_path:
        return
    with open(summary_path, "a", encoding=ENCODING_UTF8) as f:
        f.write(markdown + "\n")


def accent_pair(mode: dict[str, object]) -> tuple[str, str]:
    accent = mode[TOKEN_KEY_ACCENT]
    assert isinstance(accent, dict)
    return str(accent[TOKEN_ACCENT_TEAL]), str(accent[TOKEN_ACCENT_ORANGE])
