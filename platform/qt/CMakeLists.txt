cmake_minimum_required(VERSION 3.21)

project(calumma-qt LANGUAGES CXX)

# C++20, required rather than "if available". C++23 modules are deliberately out — see
# docs/plans/01-qt-shell.md.
set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)
set(CMAKE_AUTOMOC ON)

set(CALUMMA_ENGINE_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../../engine")

# The engine and both shells share one C ABI header — platform/shared/Calumma.hpp — including
# the canonical wire-value enums (CalmTool, CalmBlendMode, …) neither shell hand-duplicates.
set(CALUMMA_HEADER_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../shared"
    CACHE PATH "Directory containing Calumma.hpp")

# Release by default, the same choice manage.py makes for the macOS shell: every configuration
# links one optimized engine, because a debug engine is a different engine to draw with. The
# override exists so a smoke run does not have to wait for an LTO build.
set(CALUMMA_CARGO_PROFILE "release" CACHE STRING "Cargo profile for the engine (release/debug)")

if(CALUMMA_CARGO_PROFILE STREQUAL "release")
    set(CALUMMA_CARGO_FLAGS --release)
else()
    set(CALUMMA_CARGO_FLAGS "")
endif()
set(CALUMMA_FFI_DIR "${CALUMMA_ENGINE_DIR}/target/${CALUMMA_CARGO_PROFILE}")
if(WIN32)
    set(CALUMMA_FFI_LIB "${CALUMMA_FFI_DIR}/calumma_ffi.lib")
else()
    set(CALUMMA_FFI_LIB "${CALUMMA_FFI_DIR}/libcalumma_ffi.a")
endif()

add_custom_command(
    OUTPUT "${CALUMMA_FFI_LIB}"
    COMMAND cargo build ${CALUMMA_CARGO_FLAGS} -p calumma-ffi
    WORKING_DIRECTORY "${CALUMMA_ENGINE_DIR}"
    COMMENT "cargo build -p calumma-ffi (${CALUMMA_CARGO_PROFILE})"
    VERBATIM)
add_custom_target(calumma_ffi_build DEPENDS "${CALUMMA_FFI_LIB}")

add_library(calumma_ffi STATIC IMPORTED)
set_target_properties(calumma_ffi PROPERTIES IMPORTED_LOCATION "${CALUMMA_FFI_LIB}")

# What a Rust staticlib needs from the platform on top of its own objects. If a link fails on a
# symbol that is none of ours, ask rustc for the current list rather than guessing at it:
#   cargo rustc --release -p calumma-ffi --crate-type staticlib -- --print native-static-libs
if(APPLE)
    find_package(Threads REQUIRED)
    target_link_libraries(calumma_ffi INTERFACE Threads::Threads ${CMAKE_DL_LIBS}
        "-framework CoreFoundation" "-framework Foundation" "-framework Metal"
        "-framework QuartzCore" "-framework IOKit" "-framework IOSurface"
        "-framework CoreGraphics" "-framework CoreText" "-framework AppKit")
elseif(UNIX)
    find_package(Threads REQUIRED)
    target_link_libraries(calumma_ffi INTERFACE Threads::Threads ${CMAKE_DL_LIBS} m)
elseif(WIN32)
    target_link_libraries(calumma_ffi INTERFACE
        ws2_32 userenv ntdll bcrypt advapi32 kernel32 user32 dxgi d3d12 dxguid)
endif()

# The FFI wrapper deliberately does not depend on Qt. It is the half of the shell that can be
# built and run anywhere the engine builds — including macOS, where there is no Qt app to speak
# of — which is what makes `calumma_smoke` below able to exercise it against the real engine.
add_library(calumma_engine STATIC
    src/app/AppState.cpp
    src/l10n/Catalog.cpp
    src/engine/Engine.cpp
    src/engine/EngineClipboard.cpp
    src/engine/EngineExport.cpp
    src/engine/EngineGuides.cpp
    src/engine/EngineInput.cpp
    src/engine/EngineLayers.cpp
    src/engine/EngineOps.cpp
    src/engine/EngineProjects.cpp
    src/engine/EngineText.cpp
    src/engine/EngineToolGate.cpp
    src/engine/EngineTools.cpp
    src/engine/EngineTransform.cpp
    src/engine/EngineVector.cpp
    src/engine/Limits.cpp)
add_dependencies(calumma_engine calumma_ffi_build)
target_include_directories(calumma_engine PUBLIC "${CALUMMA_HEADER_DIR}" src)
target_link_libraries(calumma_engine PUBLIC calumma_ffi)

# Warnings on, everywhere. This is the side of the ABI with no compiler checking the other
# half's promises, so the little the compiler *can* see is worth hearing about.
if(MSVC)
    add_compile_options(/W4)
else()
    add_compile_options(-Wall -Wextra)
endif()

add_executable(calumma_smoke tests/smoke.cpp)
target_link_libraries(calumma_smoke PRIVATE calumma_engine)
enable_testing()
add_test(NAME smoke COMMAND calumma_smoke)

# 6.8 LTS is the floor: enough for C++20 and Qt Quick, which the chrome needs. Optional so the
# wrapper and its smoke test still build on a machine with no Qt — the GUI is what needs it.
find_package(Qt6 6.8 QUIET COMPONENTS Gui Quick QuickControls2 Svg)
if(NOT Qt6_FOUND)
    message(STATUS "Qt6 not found — building the engine wrapper and its smoke test only")
    return()
endif()

add_executable(calumma
    src/main.cpp
    src/canvas/BoardWindow.cpp
    src/canvas/NativeSurface.cpp
    src/app/AppController.cpp
    src/models/LayerListModel.cpp
    src/models/ProjectListModel.cpp
    src/theme/ThemeBridge.cpp
    src/l10n/L10nBridge.cpp)
target_link_libraries(calumma PRIVATE
    Qt6::Gui Qt6::Quick Qt6::QuickControls2 Qt6::Svg calumma_engine)

# Three resource trees, globbed rather than hand-listed as a .qrc: the icon set alone is 50+
# files and grows without this file changing. A glob only re-runs at configure time, so a
# brand-new icon needs one `cmake` re-run (touching CMakeLists.txt, or just re-configuring) to
# be picked up — the same tradeoff every glob-based CMake source list already makes.
file(GLOB CALUMMA_QML_FILES CONFIGURE_DEPENDS "${CMAKE_CURRENT_SOURCE_DIR}/qml/*.qml"
    "${CMAKE_CURRENT_SOURCE_DIR}/qml/calm/*.qml")
file(GLOB CALUMMA_ICON_FILES CONFIGURE_DEPENDS
    "${CMAKE_CURRENT_SOURCE_DIR}/../../design/icons/*.svg")
file(GLOB CALUMMA_TRANSLATION_FILES CONFIGURE_DEPENDS
    "${CMAKE_CURRENT_SOURCE_DIR}/../../translations/*.json")

qt_add_resources(calumma "calumma_qml" PREFIX "/qml"
    BASE "${CMAKE_CURRENT_SOURCE_DIR}/qml" FILES ${CALUMMA_QML_FILES})
qt_add_resources(calumma "calumma_icons" PREFIX "/icons"
    BASE "${CMAKE_CURRENT_SOURCE_DIR}/../../design/icons" FILES ${CALUMMA_ICON_FILES})
qt_add_resources(calumma "calumma_translations" PREFIX "/translations"
    BASE "${CMAKE_CURRENT_SOURCE_DIR}/../../translations" FILES ${CALUMMA_TRANSLATION_FILES})
