#!/usr/bin/env python3

from __future__ import annotations

from pathlib import Path

CLI_DIR = Path(__file__).resolve().parent
ROOT = CLI_DIR.parent

DIR_ENGINE = "engine"
DIR_DESIGN = "design"
DIR_DOCS = "docs"
DIR_PLATFORM = "platform"
DIR_MACOS = "macos"
DIR_CALUMMA = "Calumma"
DIR_UI = "UI"
DIR_THEME = "Theme"
DIR_TARGET = "target"
DIR_TRANSLATIONS = "translations"
DIR_DIST = "dist"
DIR_ASSETS = "Assets.xcassets"
DIR_APPICON = "AppIcon.appiconset"

FILE_CARGO_TOML = "Cargo.toml"
FILE_CARGO_LOCK = "Cargo.lock"
FILE_TOKENS_JSON = "tokens.json"
FILE_STYLE_MD = "STYLE.md"
FILE_SWIFT_FORMAT = ".swift-format"
FILE_TOKENS_SWIFT = "Tokens.generated.swift"
FILE_XCODEPROJ = "Calumma.xcodeproj"
FILE_COVERAGE_JSON = "coverage.json"
FILE_LANG_EN = "en.json"
FILE_ICON_MASTER = "icon.png"

ENGINE = ROOT / DIR_ENGINE
ENGINE_MANIFEST = ENGINE / FILE_CARGO_TOML
ENGINE_TARGET = ENGINE / DIR_TARGET
ENGINE_LOCK = ENGINE / FILE_CARGO_LOCK
COVERAGE_JSON = ENGINE_TARGET / FILE_COVERAGE_JSON

DESIGN = ROOT / DIR_DESIGN
TOKENS_PATH = DESIGN / FILE_TOKENS_JSON
DOCS = ROOT / DIR_DOCS
STYLE_PATH = DOCS / FILE_STYLE_MD
ICON_MASTER = DESIGN / FILE_ICON_MASTER

TRANSLATIONS = ROOT / DIR_TRANSLATIONS
TRANSLATIONS_EN = TRANSLATIONS / FILE_LANG_EN

MACOS = ROOT / DIR_PLATFORM / DIR_MACOS
SWIFT_FORMAT_CONFIG = MACOS / FILE_SWIFT_FORMAT
TOKENS_SWIFT_OUT = MACOS / DIR_CALUMMA / DIR_UI / DIR_THEME / FILE_TOKENS_SWIFT
XCODE_PROJECT = MACOS / FILE_XCODEPROJ
APPICON_DIR = MACOS / DIR_CALUMMA / DIR_ASSETS / DIR_APPICON
APPICON_SIZES = (16, 32, 64, 128, 256, 512, 1024)
MACOS_SOURCES = MACOS / DIR_CALUMMA
MACOS_BUILD = MACOS / "build"
MACOS_DERIVED = MACOS / "DerivedData"
MACOS_RELEASE_PRODUCTS = MACOS_DERIVED / "Build" / "Products" / "Release"

DIST = ROOT / DIR_DIST
DIST_STAGING = DIST / "dmg-root"

CRATE_DIRS = ("core", "io", "ops", "render", "ffi")
CRATE_PREFIX = "calumma-"
PKG_FFI = f"{CRATE_PREFIX}ffi"
PKG_CORE = f"{CRATE_PREFIX}core"

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
BIN_XCODEGEN = "xcodegen"
BIN_XCODEBUILD = "xcodebuild"
BIN_SWIFT_FORMAT = "swift-format"
BIN_TAPLO = "taplo"
BIN_OPEN = "open"
BIN_CARGO_LLVM_COV = "cargo-llvm-cov"
BIN_CARGO_OUTDATED = "cargo-outdated"
BIN_CARGO_AUDIT = "cargo-audit"
BIN_CARGO_DENY = "cargo-deny"
BIN_HDIUTIL = "hdiutil"
BIN_CODESIGN = "codesign"
BIN_DITTO = "ditto"

SCHEME_CALUMMA = "Calumma"
CONFIG_DEBUG = "Debug"
CONFIG_RELEASE = "Release"
DEST_MACOS = "platform=macOS"

APP_NAME = "Calumma"
APP_BUNDLE = f"{APP_NAME}.app"
APPLICATIONS_LINK = "Applications"
APPLICATIONS_TARGET = "/Applications"
DMG_FORMAT = "UDZO"
DMG_SUFFIX = ".dmg"
CHECKSUM_SUFFIX = ".sha256"
SIGN_IDENTITY_ADHOC = "-"
SIGN_OPTIONS_RUNTIME = "runtime"

MSG_WROTE = "wrote"
MSG_CORE_CLEAN = "calumma-core dependency tree is clean"
MSG_CORE_SKIP = "calumma-core not in workspace yet — skipping purity check"
MSG_CORE_DIRTY = "calumma-core must stay free of platform/GPU deps:"
MSG_INSTALL_LLVM_COV = "installing cargo-llvm-cov…"
MSG_NO_COVERAGE = "no coverage rows parsed; raw summary:"
MSG_DENY_SKIP = "cargo-deny not installed; skip"
MSG_COVERAGE_TOTAL = "total"
MSG_N_A = "n/a"
MSG_NO_APP = "release build produced no app bundle at"
MSG_NO_ICON_MASTER = "no app icon master found at"
MSG_PACKAGED = "packaged"
MSG_SIGNED_ADHOC = "ad-hoc signed (no Developer ID); Gatekeeper will require right-click → Open"

ENV_GITHUB_OUTPUT = "GITHUB_OUTPUT"
OUTPUT_KEY_DMG = "dmg"
OUTPUT_KEY_VERSION = "version"

ENV_GITHUB_STEP_SUMMARY = "GITHUB_STEP_SUMMARY"

ENV_CALUMMA_VERSION = "CALUMMA_VERSION"
# Read by the "Stamp version from Cargo.toml" Xcode build phase (platform/macos/project.yml)
# to override its Cargo.toml self-heal with a git-tag-resolved release version.
ENV_CALUMMA_VERSION_OVERRIDE = "CALUMMA_VERSION_OVERRIDE"

# GitHub Actions sets these automatically for every step; see
# https://docs.github.com/actions/learn-github-actions/variables#default-environment-variables
ENV_GITHUB_REF_TYPE = "GITHUB_REF_TYPE"
ENV_GITHUB_REF_NAME = "GITHUB_REF_NAME"
ENV_GITHUB_SHA = "GITHUB_SHA"
ENV_RUNNER_TEMP = "RUNNER_TEMP"
REF_TYPE_TAG = "tag"

# Workflow-specific values the workflow YAML maps into these env vars.
ENV_VERSION_INPUT = "VERSION_INPUT"
ENV_AUTO_RELEASE = "AUTO_RELEASE"
ENV_RELEASE_VERSION = "VERSION"
ENV_RELEASE_DMG = "DMG"

BIN_GH = "gh"
BIN_GIT = "git"

FILE_NOTES_MD = "notes.md"

MSG_VERSION_UNCHANGED = "workspace version unchanged"
MSG_VERSION_BUMPED = "workspace version bumped to"

ENCODING_UTF8 = "utf-8"

PYTHON_PATHS = ("manage.py", "cli")
