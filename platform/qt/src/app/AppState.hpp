#pragma once

#include "engine/Engine.hpp"

#include <cstdint>
#include <string>
#include <vector>

namespace calumma {

// Matches `AppTheme` in `Theme.swift` exactly: a plain toggle, no "follow the system" option.
// (`SettingsView.swift` only ever offers `themeLight`/`themeDark` chips — there is no third
// choice to port.)
enum class Theme : uint8_t {
    Light = 0,
    Dark = 1,
};

// Every knob the shell is allowed to hold, and nothing else. AGENTS.md draws this line: active
// tool, brush, colour, size, ink opacity, tolerance, blur strength, clone/heal aligned,
// eyedropper radius, shape fill and stroke, panel visibility, open tab ids, theme, language.
//
// What is deliberately *not* here is as much of the point: last-used shape and selection tools,
// whether a tool takes a brush size, the hex of a colour, zoom, fit, the palette, and which
// project is open all live in the engine and are read from CalmState or asked for. A field for
// any of those would be a second copy of the truth, and the two would drift.
//
// Setting a knob pushes it to the engine, because the engine is what acts on it. The value kept
// here is only so the chrome can show what is selected without asking on every repaint.
class AppState {
public:
    explicit AppState(Engine &engine);

    // Tools and ink
    uint32_t tool() const noexcept { return m_tool; }
    void setTool(uint32_t tool);
    uint32_t brushKind() const noexcept { return m_brushKind; }
    void setBrushKind(uint32_t brush);
    Rgba color() const noexcept { return m_color; }
    void setColor(Rgba color);
    Rgba strokeColor() const noexcept { return m_strokeColor; }
    void setStrokeColor(Rgba color);
    float brushSize() const noexcept { return m_brushSize; }
    void setBrushSize(float size);
    float inkOpacity() const noexcept { return m_inkOpacity; }
    void setInkOpacity(float opacity);
    uint8_t tolerance() const noexcept { return m_tolerance; }
    void setTolerance(uint8_t tolerance);
    float blurStrength() const noexcept { return m_blurStrength; }
    void setBlurStrength(float strength);
    float eraserHardness() const noexcept { return m_eraserHardness; }
    void setEraserHardness(float hardness);
    bool cloneAligned() const noexcept { return m_cloneAligned; }
    void setCloneAligned(bool aligned);
    uint32_t eyedropperRadius() const noexcept { return m_eyedropperRadius; }
    void setEyedropperRadius(uint32_t radius);
    bool shapeFill() const noexcept { return m_shapeFill; }
    void setShapeFill(bool fill);
    bool shapeStroke() const noexcept { return m_shapeStroke; }
    void setShapeStroke(bool stroke);

    // Chrome
    bool toolsPanelVisible() const noexcept { return m_toolsPanelVisible; }
    void setToolsPanelVisible(bool visible) { m_toolsPanelVisible = visible; }
    bool layersPanelVisible() const noexcept { return m_layersPanelVisible; }
    void setLayersPanelVisible(bool visible) { m_layersPanelVisible = visible; }
    Theme theme() const noexcept { return m_theme; }
    void setTheme(Theme theme);
    bool isDark() const noexcept { return m_theme == Theme::Dark; }
    const std::string &language() const noexcept { return m_language; }
    void setLanguage(std::string language) { m_language = std::move(language); }

    // Tabs. The ids are the engine's to persist — this is the shell's copy of the row the
    // titlebar draws, kept in step with it.
    const std::vector<std::string> &openTabs() const noexcept { return m_openTabs; }
    void restoreTabs();
    void addTab(const std::string &id);
    void closeTab(const std::string &id);
    void clearTabs();

    // Pushes the board colours for the current theme, so no colour is hardcoded in Rust or
    // WGSL — the one direction tokens travel into the engine.
    void applyBoardColors();

    // Sets every knob to the engine's own default, which is where defaults live.
    void resetToEngineDefaults();

private:
    Engine &m_engine;

    uint32_t m_tool = 0;
    uint32_t m_brushKind = 0;
    Rgba m_color{20, 20, 20, 255};
    Rgba m_strokeColor{20, 20, 20, 255};
    float m_brushSize = 0.0f;
    float m_inkOpacity = 1.0f;
    uint8_t m_tolerance = 0;
    float m_blurStrength = 0.0f;
    float m_eraserHardness = 0.0f;
    bool m_cloneAligned = false;
    uint32_t m_eyedropperRadius = 0;
    bool m_shapeFill = true;
    bool m_shapeStroke = true;

    bool m_toolsPanelVisible = true;
    bool m_layersPanelVisible = true;
    Theme m_theme = Theme::Light;
    std::string m_language = "en";
    std::vector<std::string> m_openTabs;
};

}  // namespace calumma
