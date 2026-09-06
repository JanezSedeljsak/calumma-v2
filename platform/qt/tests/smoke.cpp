// Drives the FFI wrapper against a real engine, with no window and no Qt. Phase 2 of
// docs/plans/01-qt-shell.md asks for exactly this — create a project, read state, undo — and
// it is the only part of the shell that can be run on the machine the engine is developed on,
// so it covers rather more than the minimum.
//
// Nothing here attaches a surface: strokes and layer edits are document work, which the engine
// does whether or not anything is drawing it. That is what makes a headless check meaningful.

#include "app/AppState.hpp"
#include "app/Shortcuts.hpp"
#include "engine/Engine.hpp"
#include "engine/Limits.hpp"
#include "l10n/Catalog.hpp"
#include "theme/Tokens.generated.hpp"

#include <cstdio>
#include <filesystem>
#include <string>
#include <vector>

namespace {

int failures = 0;

void check(bool condition, const std::string &what) {
    if (!condition) {
        ++failures;
    }
    std::printf("%s %s\n", condition ? "ok  " : "FAIL", what.c_str());
}

}  // namespace

int main() {
    // Never the default store: that is the developer's own projects.
    const std::filesystem::path dir =
        std::filesystem::temp_directory_path() / "calumma-qt-smoke";
    std::error_code ec;
    std::filesystem::remove_all(dir, ec);
    std::filesystem::create_directories(dir, ec);
    const std::string db = (dir / "smoke.sqlite").string();

    {
        calumma::Engine engine(db);
        check(engine.isValid(), "the engine opens a store of its own");

        const std::string id = engine.createProject("Smoke", 640, 480);
        check(!id.empty(), "creating a project answers with its id");

        CalmState state = engine.state();
        check(state.width == 640 && state.height == 480, "the document is the size it was asked for");
        check(state.can_undo == 0, "a fresh project has nothing to undo");

        // A new document arrives with Paper underneath a first empty layer, so the stack starts
        // at two and the active layer is the one above Paper.
        const uint32_t startingLayers = state.layer_count;
        check(startingLayers == 2, "a new project starts with Paper and one layer");
        check(engine.layerIsPaper(0), "the bottom layer is Paper");
        check(!engine.layerIsPaper(1), "the layer above it is not");
        check(engine.layerName(0) == "Paper", "Paper is named by the engine, not the shell");
        check(engine.layerVisible(1), "a new layer is visible");

        check(engine.addLayer(), "adding a layer");
        check(engine.state().layer_count == startingLayers + 1, "the stack grew");
        check(engine.state().can_undo == 1, "adding a layer is undoable");
        check(engine.undo(), "undo");
        check(engine.state().layer_count == startingLayers, "undo put the stack back");

        check(engine.redo(), "redo");
        check(engine.state().layer_count == startingLayers + 1, "redo re-added it");
        check(engine.undo(), "undo again, back to two layers");

        check(engine.setLayerName(1, "Sketch"), "renaming a layer");
        check(engine.layerName(1) == "Sketch", "the new name reads back");
        check(engine.setLayerVisible(1, false), "hiding a layer");
        check(!engine.layerVisible(1), "it reads back hidden");
        check(engine.setLayerVisible(1, true), "showing it again");
        check(engine.setLayerOpacity(1, 0.5f), "setting layer opacity");
        check(engine.layerOpacity(1) > 0.49f && engine.layerOpacity(1) < 0.51f,
              "the opacity reads back");
        check(engine.setActiveLayer(1), "selecting a layer");
        check(engine.state().active_layer == 1, "it is the active one");

        // Viewport and camera, without a surface: resize is what a window would report, and the
        // engine's own fit decides the zoom.
        check(engine.resize(1280, 800, 1.0f), "the viewport takes a size");
        const calumma::Viewport vp = engine.viewport();
        check(vp.width == 1280.0f && vp.height == 800.0f, "the engine kept that viewport");
        check(engine.fit(), "fit");
        check(engine.state().is_fit == 1, "the engine says it is fitted");
        const float fitted = engine.state().zoom;
        check(fitted > 0.0f, "fit chose a zoom");
        check(engine.stepZoom(true), "zooming in a step");
        check(engine.state().zoom > fitted, "the zoom went up");
        check(engine.state().is_fit == 0, "and it is no longer fit");
        check(engine.pan(10.0f, -5.0f), "panning");
        check(engine.endCameraMotion(), "ending the camera motion");

        // Tool knobs, then an actual stroke. The engine owns what a stroke does; this only
        // proves the wrapper's calls land and the document changed as a result.
        check(engine.setTool(CalmToolPen), "picking the pen");
        check(engine.setColor({255, 0, 0, 255}), "setting the ink colour");
        check(engine.setBrush(24.0f), "setting the brush size");
        check(engine.setInkOpacity(1.0f), "setting ink opacity");
        check(engine.setTool(CalmToolEraser), "picking the eraser");
        check(engine.setEraserHardness(0.5f), "setting eraser hardness");
        check(engine.setTool(CalmToolPen), "back to the pen");

        // The stroke goes in a project of its own, so "a stroke is undoable" is measured
        // against an empty history rather than against everything done above it.
        const std::string strokeId = engine.createProject("Stroke", 640, 480);
        check(!strokeId.empty(), "a second project for the stroke");
        check(engine.resize(1280, 800, 1.0f) && engine.fit(), "sized and fitted");
        check(engine.state().can_undo == 0, "its history starts empty");
        check(engine.pointerDown(400.0f, 300.0f), "pointer down");
        check(engine.pointerMove(420.0f, 320.0f), "pointer move");
        check(engine.pointerMove(440.0f, 300.0f), "pointer move again");
        check(engine.pointerUp(440.0f, 300.0f), "pointer up");
        check(engine.state().stroke_active == 0, "the stroke finished");
        check(engine.state().can_undo == 1, "the stroke is the thing there is to undo");
        check(engine.undo(), "undoing the stroke");
        check(engine.state().can_undo == 0, "and the history is empty again");
        check(engine.deleteProject(strokeId), "deleting the stroke project");
        check(engine.openProject(id), "back to the first project");

        // The rest of what BoardWindow calls on a live board. These are camera and hover
        // knobs — there is nothing to assert about where they land without a surface, but a
        // wrapper that passed the wrong argument or the wrong pointer would fail here.
        check(engine.setPointerHover(100.0f, 120.0f), "hover");
        check(engine.clearPointerHover(), "hover cleared");
        check(engine.panScroll(12.0f, -8.0f, true), "precise scroll pans");
        check(engine.panScroll(0.0f, 1.0f, false), "a wheel notch pans");
        check(engine.zoomScroll(320.0f, 240.0f, 2.0f, false), "a wheel notch zooms");
        check(engine.zoom(320.0f, 240.0f, 1.1f), "pinch zooms");
        check(engine.setShift(true) && engine.setShift(false), "shift is a knob");
        check(engine.endCameraMotion(), "camera motion ends");
        check(engine.render(), "rendering with no surface is a no-op, not a failure");
        // Zero is the engine saying "your ceiling, not mine" — it names a floor it can live
        // with and never a number that would pin a fast panel to a slow rate. With no surface
        // attached there is nothing for it to have an opinion about.
        check(engine.frameHint() == 0, "no surface means no frame-rate opinion");

        const std::vector<calumma::ProjectInfo> list = engine.projects();
        bool found = false;
        for (const calumma::ProjectInfo &info : list) {
            found = found || (info.id == id && info.name == "Smoke" && info.width == 640);
        }
        check(found, "the project is in the store's list");

        // Guides, and the ruler ticks the ruler view draws without doing any spacing maths.
        check(engine.guideCount() == 0, "a new project has no guides");
        check(engine.addGuide(0, 100.0f), "adding a guide");
        check(engine.addGuide(1, 200.0f), "adding one on the other axis");
        check(engine.guideCount() == 2, "both are there");
        check(engine.guides().size() == 2, "and both come back in the list");
        check(engine.setGuidePosition(0, 150.0f), "moving a guide");
        check(engine.setGuideColor(0, calumma::limits::defaultGuideColor()), "recolouring it");
        check(engine.removeGuide(0), "removing one");
        check(engine.guideCount() == 1, "one left");
        check(engine.clearGuides() && engine.guideCount() == 0, "clearing the rest");
        check(!engine.rulerTicksX().empty(), "the engine has ruler ticks to draw");

        // Selection and the clipboard.
        check(!engine.hasSelection(), "nothing is selected to start with");
        check(engine.selectAll(), "select all");
        check(engine.hasSelection(), "something is selected now");
        check(engine.invertSelection(), "invert");
        check(engine.deselect(), "deselect");
        check(!engine.hasSelection(), "and nothing is selected again");
        check(engine.selectAll(), "select all again, to copy from");
        const calumma::Clipping copied = engine.copy();
        check(!copied.bytes.empty(), "copy answers with bytes");
        check(engine.deselect(), "deselect after copying");

        // Pixels and files out. Every encoder is the engine's.
        const calumma::Image composite = engine.compositeRgba();
        check(composite.valid() && composite.width == 640, "the composite comes back at size");
        check(composite.rgba.size() == 640u * 480u * 4u, "and with the pixels to match");
        check(engine.layerThumbnail(1, 64).valid(), "a layer thumbnail");
        check(!engine.exportImage(CalmImageFormatPng).empty(), "PNG export");
        check(!engine.exportPsd().empty(), "PSD export");
        check(!engine.exportPdf(calumma::limits::pdfDefaultDpi()).empty(), "PDF export");
        check(!engine.exportSvg().empty(), "SVG export");

        // Layer geometry and adjustments.
        calumma::Bounds bounds;
        check(engine.layerBounds(0, bounds), "Paper has bounds");
        check(bounds.width > 0.0f, "and they are not empty");
        CalmAdjustments adjustments = engine.layerAdjustments(1);
        adjustments.brightness = 0.2f;
        check(engine.setLayerAdjustments(1, adjustments), "adjusting a layer");
        check(engine.layerAdjustments(1).brightness > 0.19f, "the adjustment reads back");
        check(!engine.layerIsText(1) && !engine.layerIsVector(1), "a raster layer is neither");
        check(!engine.layerId(1).empty(), "a layer has an id of its own");

        // Vector mode and the tool gate.
        check(engine.setVectorMode(true), "turning vector mode on");
        check(engine.vectorMode(), "it is on");
        check(engine.setVectorMode(false), "and off again");
        check(engine.selectedVectorItem() == -1, "with nothing selected");
        check(engine.toolBlocks(24).size() == 24, "the gate answers for every tool");
        check(engine.toolBlock(CalmToolPen) == 0, "the pen is not blocked on a raster layer");

        // The engine's own numbers, which the shell reads instead of holding opinions.
        check(calumma::limits::brushSize().max > calumma::limits::brushSize().min,
              "the brush has a range");
        check(calumma::limits::takesBrushSize(CalmToolPen), "the pen takes a brush size");
        check(!calumma::limits::takesBrushSize(11), "the eyedropper does not");
        check(!calumma::limits::palette().empty(), "there is a project palette");
        uint32_t parsed = 0;
        // Parsing takes a `#` or goes without; formatting always answers bare uppercase, and
        // whether a field shows a `#` in front of it is the shell's business.
        check(calumma::limits::parseHexRgb("#ff8800", parsed), "hex parses");
        check(calumma::limits::formatHexRgb(parsed) == "FF8800", "and formats back");
        check(calumma::limits::fontFamilyCount() > 0, "the engine found fonts");
        check(!calumma::limits::fontFamilyName(0).empty(), "and can name one");
        float fitW = 0.0f;
        float fitH = 0.0f;
        check(calumma::limits::fitSize(1280.0f, 800.0f, 640.0f, 480.0f, fitW, fitH) && fitW > 0.0f,
              "a document fits a viewport without an engine");

        check(engine.renameProject(id, "Renamed"), "renaming the project");
        check(engine.closeProject(), "closing the project");
        check(engine.openProject(id), "opening it again");
        check(engine.state().width == 640, "it came back the same size");
        check(engine.layerName(1) == "Sketch", "and with the layer name it was saved with");
        check(engine.deleteProject(id), "deleting the project");

        // --- the shell's own state ---------------------------------------------------------
        // Every knob starts at the engine's default rather than at one written here, and
        // setting one pushes it through. What is *not* on AppState matters as much: zoom, fit,
        // last-used tools and the palette are all read from the engine instead.
        calumma::AppState app(engine);
        check(app.brushSize() == calumma::limits::brushSize().fallback,
              "the brush starts at the engine's default, not the shell's");
        check(app.tolerance() == calumma::limits::toleranceDefault(),
              "and so does flood tolerance");
        app.setTool(CalmToolEraser);
        check(app.tool() == CalmToolEraser, "the shell remembers the active tool");
        app.setBrushSize(40.0f);
        check(app.brushSize() == 40.0f, "and the brush size it set");
        app.setColor({1, 2, 3, 255});
        check(app.color().r == 1 && app.color().b == 3, "and the ink colour");

        // A plain toggle, matching AppTheme in Theme.swift exactly — there is no "follow the
        // system" third option to resolve.
        check(!app.isDark(), "a fresh AppState starts Light, same as AppTheme's default");
        app.setTheme(calumma::Theme::Dark);
        check(app.isDark(), "Dark is dark");
        app.setTheme(calumma::Theme::Light);
        check(!app.isDark(), "and Light is light");

        // Tabs are the shell's row and the engine's to persist.
        const std::string tabbed = engine.createProject("Tabbed", 64, 64);
        app.addTab(tabbed);
        check(app.openTabs().size() == 1, "a tab was opened");
        app.addTab(tabbed);
        check(app.openTabs().size() == 1, "opening the same project twice is one tab");
        calumma::AppState restored(engine);
        restored.restoreTabs();
        check(restored.openTabs().size() == 1, "the engine gave the tab back on restore");
        app.closeTab(tabbed);
        check(app.openTabs().empty(), "closing the tab");
        check(!engine.projects().empty(), "closing a tab does not delete the project");
        check(engine.deleteProject(tabbed), "and it can still be deleted on purpose");
    }

    // --- copy, from the same files the macOS shell reads ------------------------------------
    {
        calumma::l10n::Catalog catalog;
        check(catalog.loadFile("../../translations/en.json") ||
                  catalog.loadFile("translations/en.json"),
              "the catalog loads translations/en.json");
        check(catalog.size() > 100, "with all of the copy in it");
        check(catalog("brand") == "Calumma", "a key reads back");
        check(catalog("noSuchKeyAnywhere") == "noSuchKeyAnywhere",
              "a missing key answers with itself, so the hole is visible");
        check(catalog.format("layerNamed", {"3"}) == "Layer 3", "{0} is substituted");
        check(catalog.format("deleteProjectNamed", {"Trees"}).find("\"Trees\"") !=
                  std::string::npos,
              "an escaped quote in the JSON survives");

        calumma::l10n::Catalog broken;
        check(!broken.loadFile("/nowhere/at/all.json"), "a missing file is refused");
        check(!broken.loadJson("{\"nested\": {\"a\": \"b\"}}"),
              "so is a file that is not flat strings");
        check(!broken.loadJson("{\"unterminated\": \"oops}"), "and so is a truncated one");
        calumma::l10n::Catalog escapes;
        check(escapes.loadJson("{\"tab\":\"a\\tb\",\"uni\":\"\\u00e9\\u65e5\"}"),
              "escapes parse");
        check(escapes("tab") == "a\tb" && escapes("uni") == "é日", "including \\u as UTF-8");
    }

    // --- the tool-shortcut table, ported from ToolLabels.swift's byKey ---------------------
    // `CalmTool` itself now comes from the shared platform/shared/Calumma.hpp, not a locally
    // hand-duplicated enum — this block is what pins Shortcuts.hpp's *lookup logic* against it.
    {
        using namespace calumma::shortcuts;
        check(toolForKey('p') == CalmToolPen, "p is the pen");
        check(toolForKey('v') == CalmToolMove, "v is move");
        check(toolForKey('m') == CalmToolSelectRect, "m names the marquee family");
        check(!toolForKey('z'), "z is not a tool key");
        check(isMarqueeFamily(CalmToolSelectEllipse) && isMarqueeFamily(CalmToolSelectLasso),
              "the other two marquee tools are in the family too");
        check(!isMarqueeFamily(CalmToolMagicWand) && !isMarqueeFamily(CalmToolSelectColor),
              "the wand and select-color are not, and keep their own keys");
        check(keyForTool(CalmToolPen) == 'p', "the mapping inverts cleanly");
        check(keyForTool(CalmToolSelectEllipse) == 'm', "every marquee member answers 'm' back");
        check(!keyForTool(CalmToolTransform), "Transform is a chord, not a bare key");
        check(!keyForTool(CalmToolSelectColor), "so is Select Color");
        std::array<char, 3> scratch{};
        check(shortcutLabel(CalmToolPen, scratch) == "P", "the label is the uppercased key");
        check(shortcutLabel(CalmToolTransform, scratch) == "\xE2\x8C\x98T",
              "Transform prints its chord");
        check(shortcutLabel(CalmToolSelectColor, scratch) == "\xE2\x87\xA7W",
              "and so does Select Color");
        // Every tool wire value 0..21 either has a key or is one of the two named chords —
        // catches a tool added to the enum without a shortcut-table entry or a deliberate
        // exemption.
        bool everyToolAccountedFor = true;
        for (uint32_t wire = 0; wire <= 21; ++wire) {
            const auto tool = static_cast<CalmTool>(wire);
            if (!keyForTool(tool) && tool != CalmToolTransform && tool != CalmToolSelectColor) {
                everyToolAccountedFor = false;
            }
        }
        check(everyToolAccountedFor, "every tool 0..21 has a key or a named chord");
    }

    // --- tokens, generated from design/tokens.json ------------------------------------------
    {
        check(calumma::tokens::Radius::island > 0.0f, "the island radius came through");
        check(calumma::tokens::Space::xs < calumma::tokens::Space::xxl, "the spacing scale is a scale");
        check(!calumma::tokens::presets.empty(), "there are new-project presets");
        check(calumma::tokens::presets[0].width > 0, "and they have sizes");
        // What gets pushed into the engine as board colours.
        check(calumma::tokens::Dark::desk.packedRgb() != calumma::tokens::Light::desk.packedRgb(),
              "the two themes are not the same desk");
    }

    std::filesystem::remove_all(dir, ec);
    std::printf("\n%s\n", failures == 0 ? "smoke: all checks passed"
                                        : "smoke: FAILURES above");
    return failures == 0 ? 0 : 1;
}
