#!/usr/bin/env python3

from __future__ import annotations

"""Calumma v2 task runner."""

import argparse
import json
import shutil
import sys
from pathlib import Path

CLI = Path(__file__).resolve().parent / "cli"
sys.path.insert(0, str(CLI))

from check_core_purity import check_core_purity  # noqa: E402
from constants import (  # noqa: E402
    BIN_CARGO_AUDIT,
    BIN_CARGO_DENY,
    BIN_CARGO_LLVM_COV,
    BIN_CARGO_OUTDATED,
    BIN_OPEN,
    BIN_SWIFT_FORMAT,
    BIN_TAPLO,
    BIN_XCODEBUILD,
    BIN_XCODEGEN,
    CONFIG_RELEASE,
    COVERAGE_JSON,
    DEST_MACOS,
    DIST,
    ENCODING_UTF8,
    FILE_CARGO_TOML,
    MACOS,
    MACOS_BUILD,
    MACOS_DERIVED,
    MACOS_SOURCES,
    MSG_COVERAGE_TOTAL,
    MSG_DENY_SKIP,
    MSG_INSTALL_LLVM_COV,
    MSG_NO_COVERAGE,
    MSG_WROTE,
    PKG_FFI,
    SCHEME_CALUMMA,
)
from gen_tokens_swift import generate_tokens_swift  # noqa: E402
from package_macos import package_macos  # noqa: E402
from _helpers import (  # noqa: E402
    ENGINE,
    ENGINE_LOCK,
    SWIFT_FORMAT_CONFIG,
    XCODE_PROJECT,
    cargo_cmd,
    crate_for_path,
    ensure_engine_env,
    format_coverage_markdown,
    format_pct,
    print_coverage_table,
    run,
    which,
    write_github_summary,
)


def cmd_tokens(_: argparse.Namespace) -> int:
    out = generate_tokens_swift()
    print(f"{MSG_WROTE} {out}")
    return 0


def cmd_purity(_: argparse.Namespace) -> int:
    return check_core_purity()


def xcodegen_generate() -> None:
    if which(BIN_XCODEGEN):
        run([BIN_XCODEGEN, "generate"], cwd=MACOS)


def cmd_dev(_: argparse.Namespace) -> int:
    generate_tokens_swift()
    run(cargo_cmd("build", "-p", PKG_FFI))
    xcodegen_generate()
    run([BIN_OPEN, str(XCODE_PROJECT)])
    return 0


def cmd_build(_: argparse.Namespace) -> int:
    generate_tokens_swift()
    run(cargo_cmd("build", "--release", "-p", PKG_FFI))
    xcodegen_generate()
    run(
        [
            BIN_XCODEBUILD,
            "-project",
            str(XCODE_PROJECT),
            "-scheme",
            SCHEME_CALUMMA,
            "-configuration",
            CONFIG_RELEASE,
            "build",
        ]
    )
    return 0


def cmd_package(args: argparse.Namespace) -> int:
    generate_tokens_swift()
    run(cargo_cmd("build", "--release", "-p", PKG_FFI))
    xcodegen_generate()
    package_macos(args.version)
    return 0


def cmd_test(_: argparse.Namespace) -> int:
    run(cargo_cmd("test", "--workspace"))
    return 0


def cmd_test_swift(_: argparse.Namespace) -> int:
    generate_tokens_swift()
    run(cargo_cmd("build", "-p", PKG_FFI))
    xcodegen_generate()
    run(
        [
            BIN_XCODEBUILD,
            "test",
            "-project",
            str(XCODE_PROJECT),
            "-scheme",
            SCHEME_CALUMMA,
            "-destination",
            DEST_MACOS,
        ]
    )
    return 0


def cmd_fmt(_: argparse.Namespace) -> int:
    run(cargo_cmd("fmt", "--all"))
    if which(BIN_SWIFT_FORMAT):
        run(
            [
                BIN_SWIFT_FORMAT,
                "format",
                "--configuration",
                str(SWIFT_FORMAT_CONFIG),
                "-i",
                "-r",
                str(MACOS_SOURCES),
            ]
        )
    if which(BIN_TAPLO):
        tomls = [str(ENGINE / FILE_CARGO_TOML), *[str(p) for p in ENGINE.glob(f"*/{FILE_CARGO_TOML}")]]
        run([BIN_TAPLO, "fmt", *tomls])
    return 0


def cmd_lint(_: argparse.Namespace) -> int:
    run(cargo_cmd("clippy", "--workspace", "--all-targets", "--", "-D", "warnings"))
    if which(BIN_SWIFT_FORMAT):
        run(
            [
                BIN_SWIFT_FORMAT,
                "lint",
                "--configuration",
                str(SWIFT_FORMAT_CONFIG),
                "-s",
                "-r",
                str(MACOS_SOURCES),
            ]
        )
    return check_core_purity()


def cmd_check(args: argparse.Namespace) -> int:
    if cmd_fmt(args):
        return 1
    if cmd_lint(args):
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
                "lines": format_pct(b["lines_covered"], b["lines_count"]),
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
                    "lines": format_pct(int(lines.get("covered", 0)), int(lines.get("count", 0))),
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
    for path in (MACOS_BUILD, MACOS_DERIVED, DIST):
        if path.exists():
            shutil.rmtree(path)
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="manage.py",
        description="Calumma v2 task runner (tokens, engine, coverage, macOS).",
    )
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("tokens", help="regenerate Swift tokens from design/tokens.json").set_defaults(
        func=cmd_tokens
    )
    sub.add_parser("purity", help="assert calumma-core has no platform/GPU deps").set_defaults(
        func=cmd_purity
    )
    sub.add_parser("dev", help="build ffi, generate Xcode project, open it").set_defaults(
        func=cmd_dev
    )
    sub.add_parser("build", help="release build of app").set_defaults(func=cmd_build)
    package_parser = sub.add_parser("package", help="release build + ad-hoc sign + dist/*.dmg")
    package_parser.add_argument(
        "--version", help="version stamped into the app and dmg name (default: workspace version)"
    )
    package_parser.set_defaults(func=cmd_package)
    sub.add_parser("test", help="cargo test --workspace").set_defaults(func=cmd_test)
    sub.add_parser("test-swift", help="xcodebuild test").set_defaults(func=cmd_test_swift)
    sub.add_parser("fmt", help="rustfmt (+ swift-format / taplo when present)").set_defaults(
        func=cmd_fmt
    )
    sub.add_parser("lint", help="clippy + purity (+ swift-format lint)").set_defaults(func=cmd_lint)
    sub.add_parser("check", help="fmt + lint + test").set_defaults(func=cmd_check)
    coverage_parser = sub.add_parser(
        "coverage", help="llvm-cov workspace (or one -p crate) + tiny %% table"
    )
    coverage_parser.add_argument(
        "-p", "--package", help="scope coverage to a single crate, e.g. calumma-core"
    )
    coverage_parser.set_defaults(func=cmd_coverage)
    sub.add_parser("outdated", help="cargo outdated + audit").set_defaults(func=cmd_outdated)
    sub.add_parser("clean", help="cargo clean + macOS build dirs").set_defaults(func=cmd_clean)
    return parser


def main(argv: list[str] | None = None) -> int:
    ensure_engine_env()
    parser = build_parser()
    args = parser.parse_args(argv)
    return int(args.func(args))


if __name__ == "__main__":
    raise SystemExit(main())
