#include "AppState.hpp"

#include "engine/Limits.hpp"
#include "theme/Tokens.generated.hpp"

#include <algorithm>

namespace calumma {

AppState::AppState(Engine &engine) : m_engine(engine) { resetToEngineDefaults(); }

void AppState::resetToEngineDefaults() {
    // Read, never invented. A shell that wrote its own starting brush size would be a second
    // opinion about what a brush is, and the two would drift the first time the engine changed.
    setBrushSize(limits::brushSize().fallback);
    setInkOpacity(limits::inkOpacity().fallback);
    setBlurStrength(limits::blurStrength().fallback);
    setEraserHardness(limits::eraserHardness().fallback);
    setTolerance(limits::toleranceDefault());
    setEyedropperRadius(limits::eyedropperRadiusDefault());
    setCloneAligned(limits::cloneAlignedDefault());
    setShapeFill(m_shapeFill);
    setShapeStroke(m_shapeStroke);
    setColor(m_color);
    setStrokeColor(m_strokeColor);
    setTool(m_tool);
    setBrushKind(m_brushKind);
}

void AppState::setTool(uint32_t tool) {
    m_tool = tool;
    m_engine.setTool(tool);
}

void AppState::setBrushKind(uint32_t brush) {
    m_brushKind = brush;
    m_engine.setBrushKind(brush);
}

void AppState::setColor(Rgba color) {
    m_color = color;
    m_engine.setColor(color);
}

void AppState::setStrokeColor(Rgba color) {
    m_strokeColor = color;
    m_engine.setStrokeColor(color);
}

void AppState::setBrushSize(float size) {
    m_brushSize = size;
    m_engine.setBrush(size);
}

void AppState::setInkOpacity(float opacity) {
    m_inkOpacity = opacity;
    m_engine.setInkOpacity(opacity);
}

void AppState::setTolerance(uint8_t tolerance) {
    m_tolerance = tolerance;
    m_engine.setTolerance(tolerance);
}

void AppState::setBlurStrength(float strength) {
    m_blurStrength = strength;
    m_engine.setBlurStrength(strength);
}

void AppState::setEraserHardness(float hardness) {
    m_eraserHardness = hardness;
    m_engine.setEraserHardness(hardness);
}

void AppState::setCloneAligned(bool aligned) {
    m_cloneAligned = aligned;
    m_engine.setCloneAligned(aligned);
}

void AppState::setEyedropperRadius(uint32_t radius) {
    m_eyedropperRadius = radius;
    m_engine.setEyedropperRadius(radius);
}

void AppState::setShapeFill(bool fill) {
    m_shapeFill = fill;
    m_engine.setFill(fill);
}

void AppState::setShapeStroke(bool stroke) {
    m_shapeStroke = stroke;
    m_engine.setStroke(stroke);
}

void AppState::setTheme(Theme theme) {
    m_theme = theme;
    m_engine.setDark(isDark());
    applyBoardColors();
}

void AppState::applyBoardColors() {
    if (isDark()) {
        m_engine.setBoardColors(tokens::Dark::desk.packedRgb(), tokens::Dark::deskGrid.packedRgb(),
                                tokens::Dark::paperBorder.packedRgb());
        return;
    }
    m_engine.setBoardColors(tokens::Light::desk.packedRgb(), tokens::Light::deskGrid.packedRgb(),
                            tokens::Light::paperBorder.packedRgb());
}

void AppState::restoreTabs() { m_openTabs = m_engine.openTabs(); }

void AppState::addTab(const std::string &id) {
    if (id.empty() ||
        std::find(m_openTabs.begin(), m_openTabs.end(), id) != m_openTabs.end()) {
        return;
    }
    m_openTabs.push_back(id);
    m_engine.setOpenTabs(m_openTabs);
}

void AppState::closeTab(const std::string &id) {
    // Closing a tab is not deleting a project — the row goes, the project stays in the store.
    const auto at = std::find(m_openTabs.begin(), m_openTabs.end(), id);
    if (at == m_openTabs.end()) {
        return;
    }
    m_openTabs.erase(at);
    m_engine.setOpenTabs(m_openTabs);
}

void AppState::clearTabs() {
    m_openTabs.clear();
    m_engine.setOpenTabs(m_openTabs);
}

}  // namespace calumma
