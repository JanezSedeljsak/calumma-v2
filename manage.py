#!/usr/bin/env python3

from __future__ import annotations

"""Miw task runner (Calumma engine)."""

import argparse
import json
import os
import shutil
import sys
from pathlib import Path

ROOT_DIR = Path(__file__).resolve().parent
CLI = ROOT_DIR / "cli"
sys.path.insert(0, str(CLI))


def relaunch_in_venv() -> None:
    venv = ROOT_DIR / ".venv"
    python = venv / "bin" / "python"
    if not python.is_file():
        python = venv / "Scripts" / "python.exe"
    if not python.is_file():
        return
    if Path(sys.prefix).resolve() == venv.resolve():
        return
    os.execv(str(python), [str(python), *sys.argv])


relaunch_in_venv()

from _helpers import (
    ENGINE,
    ENGINE_LOCK,
    cargo_cmd,
    crate_for_path,
    ensure_engine_env,
    format_coverage_markdown,
    format_pct,
    format_pct_with_count,
    print_coverage_table,
    run,
    which,
    workspace_version,
    write_github_summary,
)
from check_core_purity import check_core_purity
from ci_lint import run_ci_lint, run_ci_test, run_clippy, run_clippy_darwin, run_fmt
from constants import (
    BIN_CARGO_AUDIT,
    BIN_CARGO_DENY,
    BIN_CARGO_LLVM_COV,
    BIN_CARGO_OUTDATED,
    COVERAGE_JSON,
    DIST,
    ENCODING_UTF8,
    FILE_CARGO_TOML,
    GUI,
    GUI_MANIFEST,
    MCP_DEVTOOLS_FEATURE,
    MCP_DEVTOOLS_PORT,
    MSG_COVERAGE_TOTAL,
    MSG_DENY_SKIP,
    MSG_INSTALL_LLVM_COV,
    MSG_NO_COVERAGE,
    ROOT,
)
from version_check import check_version_bump


def cmd_version(_: argparse.Namespace) -> int:
    print(workspace_version())
    return 0


def cmd_examples(_: argparse.Namespace) -> int:
    from prepare_example_assets import main as prepare_examples

    return prepare_examples()


def cmd_purity(_: argparse.Namespace) -> int:
    return check_core_purity()


def cmd_icon(_: argparse.Namespace) -> int:
    from generate_icon import main as generate_icon_main

    return generate_icon_main()


def cmd_dev(args: argparse.Namespace) -> int:
    cmd = ["cargo", "run", "--manifest-path", str(GUI_MANIFEST)]
    if getattr(args, "release", False):
        cmd.append("--release")
    env = None
    if getattr(args, "mcp", False):
        cmd += ["--features", MCP_DEVTOOLS_FEATURE]
        env = {"SLINT_MCP_PORT": str(MCP_DEVTOOLS_PORT)}
    run(cmd, env=env)
    return 0


def cmd_gui_check(_: argparse.Namespace) -> int:
    run(["cargo", "check", "--manifest-path", str(GUI_MANIFEST)])
    return 0


def cmd_build(_: argparse.Namespace) -> int:
    run(["cargo", "build", "--release", "--manifest-path", str(GUI_MANIFEST)])
    return 0


def cmd_test(args: argparse.Namespace) -> int:
    if getattr(args, "ci", False):
        run_ci_test()
    else:
        run(cargo_cmd("test", "--workspace"))
    return 0


def cmd_fmt(args: argparse.Namespace) -> int:
    check = not getattr(args, "write", False)
    run_fmt(check=check, include_taplo=not check)
    return 0


def cmd_clippy(args: argparse.Namespace) -> int:
    run_clippy(darwin_only=getattr(args, "darwin_only", False))
    return 0


def cmd_lint(_: argparse.Namespace) -> int:
    run_ci_lint()
    run_clippy_darwin()
    return check_core_purity()


def cmd_check(args: argparse.Namespace) -> int:
    if cmd_lint(args):
        return 1
    if cmd_gui_check(args):
        return 1
    return cmd_test(args)


def ensure_llvm_cov() -> None:
    if which(BIN_CARGO_LLVM_COV):
        return
    print(MSG_INSTALL_LLVM_COV)
    run(["cargo", "install", "cargo-llvm-cov", "--locked"])


def parse_llvm_cov_json(payload: dict[str, object]) -> list[dict[str, object]]:
    data = payload.get("data")
    if not isinstance(data, list) or not data:
        return []
    first = data[0]
    if not isinstance(first, dict):
        return []
    files = first.get("files", [])
    if not isinstance(files, list):
        files = []

    buckets: dict[str, dict[str, int]] = {}

    def bump(name: str, key: str, covered: int, count: int) -> None:
        slot = buckets.setdefault(
            name,
            {
                "lines_covered": 0,
                "lines_count": 0,
                "funcs_covered": 0,
                "funcs_count": 0,
                "regions_covered": 0,
                "regions_count": 0,
            },
        )
        slot[f"{key}_covered"] += covered
        slot[f"{key}_count"] += count

    for file_entry in files:
        if not isinstance(file_entry, dict):
            continue
        filename = str(file_entry.get("filename", ""))
        crate = crate_for_path(filename)
        if crate is None:
            continue
        summary = file_entry.get("summary")
        if not isinstance(summary, dict):
            continue
        for metric, key in (("lines", "lines"), ("functions", "funcs"), ("regions", "regions")):
            block = summary.get(metric)
            if not isinstance(block, dict):
                continue
            bump(crate, key, int(block.get("covered", 0)), int(block.get("count", 0)))

    totals = first.get("totals")
    rows: list[dict[str, object]] = []
    for crate in sorted(buckets):
        b = buckets[crate]
        rows.append(
            {
                "crate": crate,
                "lines": format_pct_with_count(b["lines_covered"], b["lines_count"]),
                "funcs": format_pct(b["funcs_covered"], b["funcs_count"]),
                "regions": format_pct(b["regions_covered"], b["regions_count"]),
            }
        )

    if isinstance(totals, dict):
        lines = totals.get("lines", {})
        funcs = totals.get("functions", {})
        regions = totals.get("regions", {})
        if isinstance(lines, dict) and isinstance(funcs, dict) and isinstance(regions, dict):
            rows.append(
                {
                    "crate": MSG_COVERAGE_TOTAL,
                    "lines": format_pct_with_count(
                        int(lines.get("covered", 0)), int(lines.get("count", 0))
                    ),
                    "funcs": format_pct(int(funcs.get("covered", 0)), int(funcs.get("count", 0))),
                    "regions": format_pct(
                        int(regions.get("covered", 0)), int(regions.get("count", 0))
                    ),
                }
            )
    return rows


def cmd_coverage(args: argparse.Namespace) -> int:
    ensure_engine_env()
    ensure_llvm_cov()
    COVERAGE_JSON.parent.mkdir(parents=True, exist_ok=True)
    scope = ["-p", args.package] if args.package else ["--workspace"]
    run(
        cargo_cmd(
            "llvm-cov",
            *scope,
            "--json",
            "--summary-only",
            "--output-path",
            str(COVERAGE_JSON),
        )
    )
    payload = json.loads(COVERAGE_JSON.read_text(encoding=ENCODING_UTF8))
    rows = parse_llvm_cov_json(payload)
    title = f"Coverage ({args.package})" if args.package else "Coverage (workspace)"
    if not rows:
        print(MSG_NO_COVERAGE)
        run(cargo_cmd("llvm-cov", "report", "--summary-only"), check=False)
        write_github_summary(f"### {title}\n\n{MSG_NO_COVERAGE}")
        return 1
    print_coverage_table(rows)
    write_github_summary(format_coverage_markdown(rows, title))
    return 0


def cmd_outdated(_: argparse.Namespace) -> int:
    if not which(BIN_CARGO_OUTDATED):
        run(["cargo", "install", "cargo-outdated", "--locked"])
    if not which(BIN_CARGO_AUDIT):
        run(["cargo", "install", "cargo-audit", "--locked"])
    run(cargo_cmd("outdated", "--workspace", "--exit-code", "1"))
    run(["cargo", "audit", "--file", str(ENGINE_LOCK)])
    if which(BIN_CARGO_DENY):
        run(["cargo", "deny", "--manifest-path", str(ENGINE / FILE_CARGO_TOML), "check"])
    else:
        print(MSG_DENY_SKIP)
    return 0


def cmd_clean(_: argparse.Namespace) -> int:
    run(cargo_cmd("clean"), check=False)
    for path in (GUI / "target", DIST, ROOT / ".ruff_cache"):
        if path.exists():
            shutil.rmtree(path)
    skip = {".venv", "venv", "target"}
    for cache in ROOT.rglob("__pycache__"):
        if skip.intersection(cache.parts):
            continue
        shutil.rmtree(cache, ignore_errors=True)
    return 0


def cmd_version_check(_: argparse.Namespace) -> int:
    check_version_bump()
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="manage.py",
        description="Miw task runner (Calumma engine, GUI shell, coverage).",
    )
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser(
        "version", help="print engine/Cargo.toml's [workspace.package] version"
    ).set_defaults(func=cmd_version)
    sub.add_parser("purity", help="assert calumma-core has no platform/GPU deps").set_defaults(
        func=cmd_purity
    )
    sub.add_parser(
        "examples",
        help="optimize README screenshots in design/example/",
    ).set_defaults(func=cmd_examples)
    sub.add_parser(
        "icon",
        help="generate a 256x256 rounded-corner app icon from design/icon.png",
    ).set_defaults(func=cmd_icon)
    dev_parser = sub.add_parser(
        "dev",
        help="build and run the GUI shell (optimized debug; --release for a shipped-like binary)",
    )
    dev_parser.add_argument(
        "--mcp",
        action="store_true",
        help=f"enable Slint's embedded MCP server on port {MCP_DEVTOOLS_PORT}",
    )
    dev_parser.add_argument(
        "--release",
        action="store_true",
        help="run a release binary (no debug assertions; closer to a bundled app)",
    )
    dev_parser.set_defaults(func=cmd_dev)
    sub.add_parser("gui-check", help="compile-check the GUI shell (no window)").set_defaults(
        func=cmd_gui_check
    )
    sub.add_parser("build", help="release build of the GUI shell").set_defaults(func=cmd_build)
    test_parser = sub.add_parser(
        "test",
        help="cargo test --workspace (use --ci for the GHA test job scope)",
    )
    test_parser.add_argument(
        "--ci",
        action="store_true",
        help="on Linux: core + render only; elsewhere: full workspace",
    )
    test_parser.set_defaults(func=cmd_test)
    fmt_parser = sub.add_parser(
        "fmt",
        help="verify formatting like CI (default); pass --write to apply rustfmt/ruff/taplo",
    )
    fmt_parser.add_argument(
        "--write",
        action="store_true",
        help="rewrite files instead of checking",
    )
    fmt_parser.set_defaults(func=cmd_fmt)
    clippy_parser = sub.add_parser(
        "clippy",
        help="clippy portable crates; on macOS also ffi/app unless --darwin-only",
    )
    clippy_parser.add_argument(
        "--darwin-only",
        action="store_true",
        help="only calumma-ffi and calumma-app (macOS test job after ubuntu lint)",
    )
    clippy_parser.set_defaults(func=cmd_clippy)
    sub.add_parser(
        "lint", help="fmt --check + clippy + ruff + purity (ubuntu lint job)"
    ).set_defaults(func=cmd_lint)
    sub.add_parser("check", help="fmt --check + lint + gui-check + test").set_defaults(
        func=cmd_check
    )
    coverage_parser = sub.add_parser(
        "coverage", help="llvm-cov workspace (or one -p crate) + tiny %% table"
    )
    coverage_parser.add_argument(
        "-p", "--package", help="scope coverage to a single crate, e.g. calumma-core"
    )
    coverage_parser.set_defaults(func=cmd_coverage)
    sub.add_parser("outdated", help="cargo outdated + audit").set_defaults(func=cmd_outdated)
    sub.add_parser("clean", help="cargo clean + GUI build dir").set_defaults(func=cmd_clean)
    sub.add_parser(
        "version-check", help="CI: diff engine/Cargo.toml's version against the previous commit"
    ).set_defaults(func=cmd_version_check)
    return parser


def main(argv: list[str] | None = None) -> int:
    ensure_engine_env()
    parser = build_parser()
    args = parser.parse_args(argv)
    return int(args.func(args))


if __name__ == "__main__":
    raise SystemExit(main())
