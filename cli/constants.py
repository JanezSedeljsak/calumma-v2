#!/usr/bin/env python3

from __future__ import annotations

from pathlib import Path

CLI_DIR = Path(__file__).resolve().parent
ROOT = CLI_DIR.parent

DIR_ENGINE = "engine"
DIR_DESIGN = "design"
DIR_DOCS = "docs"
DIR_UI_ROOT = "ui"
DIR_GUI_ROOT = "gui"
DIR_TARGET = "target"
DIR_TRANSLATIONS = "translations"
DIR_DIST = "dist"

FILE_CARGO_TOML = "Cargo.toml"
FILE_CARGO_LOCK = "Cargo.lock"
FILE_TOKENS_JSON = "tokens.json"
FILE_STYLE_MD = "STYLE.md"
FILE_COVERAGE_JSON = "coverage.json"
FILE_LANG_EN = "en.json"

ENGINE = ROOT / DIR_ENGINE
ENGINE_MANIFEST = ENGINE / FILE_CARGO_TOML
ENGINE_TARGET = ENGINE / DIR_TARGET
ENGINE_LOCK = ENGINE / FILE_CARGO_LOCK
COVERAGE_JSON = ENGINE_TARGET / FILE_COVERAGE_JSON

DESIGN = ROOT / DIR_DESIGN
TOKENS_PATH = DESIGN / FILE_TOKENS_JSON
DOCS = ROOT / DIR_DOCS
STYLE_PATH = DOCS / FILE_STYLE_MD

TRANSLATIONS = ROOT / DIR_TRANSLATIONS
TRANSLATIONS_EN = TRANSLATIONS / FILE_LANG_EN

GUI = ROOT / DIR_GUI_ROOT
GUI_MANIFEST = GUI / FILE_CARGO_TOML
CALUMMA_HEADER_DIR = ENGINE / "ffi" / "include"

MCP_DEVTOOLS_FEATURE = "mcp-devtools"
MCP_DEVTOOLS_PORT = 7883

DIST = ROOT / DIR_DIST
DIST_STAGING = DIST / "dmg-root"

CRATE_DIRS = ("core", "io", "ops", "render", "ffi")
CRATE_PREFIX = "calumma-"
PKG_FFI = f"{CRATE_PREFIX}ffi"
PKG_CORE = f"{CRATE_PREFIX}core"

CLIPPY_PORTABLE_PACKAGES = (
    f"{CRATE_PREFIX}core",
    f"{CRATE_PREFIX}text",
    f"{CRATE_PREFIX}io",
    f"{CRATE_PREFIX}ops",
    f"{CRATE_PREFIX}render",
)

CLIPPY_DARWIN_PACKAGES = (
    PKG_FFI,
    f"{CRATE_PREFIX}app",
)

TOKEN_KEY_RADIUS = "radius"
TOKEN_KEY_SPACE = "space"
TOKEN_KEY_CONTROL = "control"
TOKEN_KEY_WINDOW = "window"
TOKEN_KEY_TYPE = "type"
TOKEN_KEY_COLOR = "color"
TOKEN_KEY_PRESETS = "presets"
TOKEN_KEY_ACCENT = "accent"
TOKEN_MODE_LIGHT = "light"
TOKEN_MODE_DARK = "dark"
TOKEN_ACCENT_TEAL = "teal"
TOKEN_ACCENT_ORANGE = "orange"

COLOR_KEYS = (
    "bg",
    "surface",
    "surfaceHover",
    "text",
    "textMuted",
    "danger",
    "desk",
    "deskGrid",
    "paper",
    "paperBorder",
    "islandBorder",
    "controlBorder",
    "controlFocusBorder",
)

RADIUS_KEYS = ("sm", "md", "lg", "window", "island")
SPACE_KEYS = ("xs", "sm", "md", "lg", "xl", "xxl")
CONTROL_KEYS = ("height",)
WINDOW_KEYS = (
    "mainWidth",
    "mainHeight",
    "mainMinWidth",
    "mainMinHeight",
    "newProjectWidth",
    "newProjectHeight",
    "newProjectMinWidth",
    "newProjectMinHeight",
    "pasteMinWidth",
    "pasteMaxWidth",
    "pasteMinHeight",
    "pasteWidthRatio",
)
TYPE_KEYS = (
    ("labelSize", "label"),
    ("labelTracking", "labelTracking"),
    ("bodySize", "body"),
    ("titleSize", "title"),
    ("brandSize", "brand"),
)

ENV_CARGO_TARGET_DIR = "CARGO_TARGET_DIR"
BIN_CARGO = "cargo"
BIN_CMAKE = "cmake"
BIN_TAPLO = "taplo"
BIN_CARGO_LLVM_COV = "cargo-llvm-cov"
BIN_CARGO_OUTDATED = "cargo-outdated"
BIN_CARGO_AUDIT = "cargo-audit"
BIN_CARGO_DENY = "cargo-deny"
BIN_HDIUTIL = "hdiutil"
BIN_CODESIGN = "codesign"
BIN_DITTO = "ditto"
BIN_ICONUTIL = "iconutil"
BIN_GH = "gh"
BIN_GIT = "git"

APP_NAME = "Miw"
APP_BIN = APP_NAME
APP_BUNDLE = f"{APP_NAME}.app"
BUNDLE_ID = "app.calumma"
MACOS_MIN_VERSION = "26.0"
APPLICATIONS_LINK = "Applications"
APPLICATIONS_TARGET = "/Applications"
DMG_FORMAT = "UDZO"
DMG_SUFFIX = ".dmg"
CHECKSUM_SUFFIX = ".sha256"
SIGN_IDENTITY_ADHOC = "-"
SIGN_OPTIONS_RUNTIME = "runtime"
ICONSET_DIR_NAME = "AppIcon.iconset"
ICNS_NAME = "AppIcon.icns"

MSG_WROTE = "wrote"
MSG_CORE_CLEAN = "calumma-core dependency tree is clean"
MSG_CORE_SKIP = "calumma-core not in workspace yet — skipping purity check"
MSG_CORE_DIRTY = "calumma-core must stay free of platform/GPU deps:"
MSG_INSTALL_LLVM_COV = "installing cargo-llvm-cov…"
MSG_NO_COVERAGE = "no coverage rows parsed; raw summary:"
MSG_DENY_SKIP = "cargo-deny not installed; skip"
MSG_COVERAGE_TOTAL = "total"
MSG_N_A = "n/a"
MSG_NO_APP = "release build produced no app binary at"
MSG_PACKAGED = "packaged"
MSG_SIGNED_ADHOC = "ad-hoc signed (no Developer ID); Gatekeeper will require right-click → Open"
MSG_PACKAGE_MACOS_ONLY = "package is macOS-only (needs codesign, hdiutil, iconutil)"

ENV_GITHUB_OUTPUT = "GITHUB_OUTPUT"
ENV_GITHUB_STEP_SUMMARY = "GITHUB_STEP_SUMMARY"
ENV_GITHUB_REF_TYPE = "GITHUB_REF_TYPE"
ENV_GITHUB_REF_NAME = "GITHUB_REF_NAME"
ENV_GITHUB_REPOSITORY = "GITHUB_REPOSITORY"
ENV_GITHUB_SHA = "GITHUB_SHA"
ENV_RUNNER_TEMP = "RUNNER_TEMP"
ENV_VERSION_INPUT = "VERSION_INPUT"
ENV_AUTO_RELEASE = "AUTO_RELEASE"
ENV_RELEASE_VERSION = "VERSION"
ENV_RELEASE_DMG = "DMG"
ENV_MACOSX_DEPLOYMENT_TARGET = "MACOSX_DEPLOYMENT_TARGET"
REF_TYPE_TAG = "tag"
FILE_NOTES_MD = "notes.md"
OUTPUT_KEY_DMG = "dmg"
OUTPUT_KEY_VERSION = "version"

MSG_VERSION_UNCHANGED = "workspace version unchanged"
MSG_VERSION_BUMPED = "workspace version bumped to"
MSG_VERSION_MISMATCH = "gui/Cargo.toml version does not match engine/Cargo.toml [workspace.package]"

ENCODING_UTF8 = "utf-8"

PYTHON_PATHS = ("manage.py", "cli")
