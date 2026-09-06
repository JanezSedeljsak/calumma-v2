# platform/qt

The Windows + Linux shell, C++20 over the same C ABI the macOS app uses. macOS stays
`platform/macos` (SwiftUI + Metal); Qt is not coming to Mac, so this does not build there —
`nativeSurfaceFor` has no mapping for Darwin on purpose.

The C ABI header itself — `platform/shared/Calumma.hpp` — is shared with the Swift shell, not
copied. It also declares the canonical wire-value enums (`CalmTool`, `CalmBlendMode`,
`CalmAlignEdge`, `CalmDistributeAxis`, `CalmBrush`, `CalmCropOverlayStyle`, `CalmPasteOutcome`)
that both shells used to hand-duplicate; see `docs/plans/01-qt-shell.md`'s "The shared enum
vocabulary" for how a plain C enum was made to import into Swift as a real, exhaustively-
switchable `enum` rather than an opaque struct of constants.

`docs/plans/01-qt-shell.md` owns the design and the phase order, including a table of exactly
what's verified vs. only ever reviewed. In short: phases 1–5 are written. The engine wrapper,
`AppState`, the tool-shortcut table, the l10n catalog and the generated tokens are Qt-free and
tested (`./manage.py qt-smoke`, 165 checks against a real engine). Everything that needs Qt
itself — `AppController`, the two list models, `ThemeBridge`, `L10nBridge`, `BoardWindow`, every
file under `qml/` — has never been compiled, because there is no Qt on the machine this was
written on. Do not trust it further than that until someone runs it.

## Build

Needs CMake 3.21+ and a Rust toolchain; Qt 6.8+ (Gui, Quick, QuickControls2, Svg) only for the
GUI. `QtQuick.Effects` (`MultiEffect`, used by `ToolIcon.qml` to tint the SVG icon set) ships
with Qt Quick itself from 6.5 on — no extra module beyond `Quick`. CMake builds the engine
itself, so there is no separate cargo step.

```
cmake -G Ninja -B build -S .
cmake --build build
./build/calumma
```

On Linux run under xcb until the Wayland handle question in plan 01 is settled:

```
QT_QPA_PLATFORM=xcb ./build/calumma
```

The engine wrapper holds no Qt type, so it builds and runs anywhere the engine does — the GUI
target is simply skipped when Qt is missing. `platform/qt/tests/smoke.cpp` drives the wrapper,
`AppState`, `Shortcuts.hpp` and the generated tokens against a real engine with no window at all:

```
./manage.py qt-smoke
```

## Layout

| Path | What lives there |
| --- | --- |
| `src/engine/Engine*.cpp` | RAII C++ over `calm_engine_*`, the counterpart of `Bridge/*.swift`. No Qt |
| `src/engine/Limits.hpp` | What the ABI answers with no engine: ranges, defaults, per-tool predicates, palette, fitting |
| `src/app/AppState.*` | The shell knobs AGENTS.md allows, and nothing else. No Qt |
| `src/app/Shortcuts.hpp` | The tool-key *lookup*, ported from `ToolLabels.swift`'s `byKey`. No Qt. The `CalmTool` enum itself is `../../shared/Calumma.hpp`'s, not a local copy |
| `src/app/AppController.*` | The QObject QML actually talks to — owns `Engine`, `AppState`, the models |
| `src/models/` | `LayerListModel`, `ProjectListModel` — `QAbstractListModel`s for the layer stack and project lists |
| `src/l10n/Catalog.*` | Hand-rolled flat-JSON reader for `translations/*.json`. No Qt |
| `src/l10n/L10nBridge.*` | The `QObject` wrapper QML calls (`l10n.t("brand")`) |
| `src/theme/Tokens.generated.hpp` | Written by `./manage.py tokens`. Do not edit |
| `src/theme/ThemeBridge.*` | The generated tokens, resolved to the active palette, reachable from QML |
| `src/canvas/` | The board window, its native handles, and the events it forwards |
| `qml/` | The chrome — see the plan for exactly which screens exist and which don't |
| `src/main.cpp` | Application entry, `QQmlApplicationEngine` wiring |
| `tests/smoke.cpp` | Drives everything above the Qt line against a real engine, headless |

The engine wrapper covers the whole C ABI — every function in `Calumma.hpp` except
`calm_engine_attach_surface`, which is the macOS Metal attach this shell replaces with
`calm_engine_attach_native_surface`.

## The riskiest part

The board is a native `QWindow`, not a Quick item — wgpu needs a real platform surface, and
Quick's own scene graph can't give it one. `AppController::attachBoardHost` reparents
`BoardWindow` under the QML scene's window (`QWindow::setParent`) and keeps it positioned under
a placeholder `Item` (`BoardHost` in `Editor.qml`) by watching that item's geometry. This is a
documented Qt technique, not an invented one — but it has never been run. If the board doesn't
show up, or shows up in the wrong place, start there.
