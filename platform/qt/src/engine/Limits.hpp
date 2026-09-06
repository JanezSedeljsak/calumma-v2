#pragma once

#include <Calumma.hpp>

#include <cstdint>
#include <string>
#include <vector>

// Everything the C ABI answers without an engine: ranges, defaults, per-tool predicates, the
// palette, fitting maths. These are not methods because they are not state — they are the
// engine's rules, and the shell reads them rather than restating them. A slider that hardcoded
// its own minimum would be a second opinion about what a brush is.
namespace calumma::limits {

struct Range {
    float min = 0.0f;
    float max = 0.0f;
    float fallback = 0.0f;
};

inline Range brushSize() {
    return {calm_brush_size_min(), calm_brush_size_max(), calm_brush_size_default()};
}
inline Range inkOpacity() {
    return {calm_ink_opacity_min(), calm_ink_opacity_max(), calm_ink_opacity_default()};
}
inline Range blurStrength() {
    return {calm_blur_strength_min(), calm_blur_strength_max(), calm_blur_strength_default()};
}
inline Range eraserHardness() {
    return {calm_eraser_hardness_min(), calm_eraser_hardness_max(), calm_eraser_hardness_default()};
}
inline Range textSize() {
    return {calm_text_size_min(), calm_text_size_max(), calm_text_size_default()};
}
inline Range textLineHeight() {
    return {calm_text_line_height_min(), calm_text_line_height_max(),
            calm_text_line_height_default()};
}

// A brush slider moves in the engine's own unit, not in pixels: the mapping is not linear and
// the shell must not invent one.
inline float brushSizeToUnit(float size) { return calm_brush_size_unit(size); }
inline float brushSizeFromUnit(float unit) { return calm_brush_size_from_unit(unit); }
inline float brushSizeStep(float size, bool increase) {
    return calm_brush_size_step(size, increase ? 1 : 0);
}
inline float textSizeToUnit(float size) { return calm_text_size_unit(size); }
inline float textSizeFromUnit(float unit) { return calm_text_size_from_unit(unit); }
inline float textWrapMin() { return calm_text_wrap_min(); }

inline uint8_t toleranceMin() { return calm_tolerance_min(); }
inline uint8_t toleranceMax() { return calm_tolerance_max(); }
inline uint8_t toleranceDefault() { return calm_tolerance_default(); }
inline uint32_t eyedropperRadiusMin() { return calm_eyedropper_radius_min(); }
inline uint32_t eyedropperRadiusMax() { return calm_eyedropper_radius_max(); }
inline uint32_t eyedropperRadiusDefault() { return calm_eyedropper_radius_default(); }
inline bool cloneAlignedDefault() { return calm_clone_aligned_default() != 0; }
inline uint32_t importMaxSide() { return calm_import_max_side(); }
inline uint32_t pasteStaggerPx() { return calm_paste_stagger_px(); }
inline float lossyExportQuality() { return calm_lossy_export_quality(); }
inline float pdfDefaultDpi() { return calm_pdf_default_dpi(); }
inline size_t guidesLimit() { return calm_guides_limit(); }
inline uint32_t defaultGuideColor() { return calm_default_guide_color(); }

// What a given tool takes, so the options panel shows a control because the engine says the
// tool uses it — never because a switch statement in the shell says so.
inline bool isShape(uint32_t tool) { return calm_tool_is_shape(tool) != 0; }
inline bool isSelection(uint32_t tool) { return calm_tool_is_selection(tool) != 0; }
inline bool takesFill(uint32_t tool) { return calm_tool_takes_fill(tool) != 0; }
inline bool takesBrush(uint32_t tool) { return calm_tool_takes_brush(tool) != 0; }
inline bool takesBrushSize(uint32_t tool) { return calm_tool_takes_brush_size(tool) != 0; }
inline bool takesInkOpacity(uint32_t tool) { return calm_tool_takes_ink_opacity(tool) != 0; }
inline bool takesBlurStrength(uint32_t tool) { return calm_tool_takes_blur_strength(tool) != 0; }
inline bool takesTolerance(uint32_t tool) { return calm_tool_takes_tolerance(tool) != 0; }
inline bool takesEraserHardness(uint32_t tool) { return calm_tool_takes_eraser_hardness(tool) != 0; }
inline bool takesCloneAligned(uint32_t tool) { return calm_tool_takes_clone_aligned(tool) != 0; }
inline bool takesEyedropperRadius(uint32_t tool) {
    return calm_tool_takes_eyedropper_radius(tool) != 0;
}
inline bool showsVectorMode(uint32_t tool) { return calm_tool_shows_vector_mode(tool) != 0; }

// The project palette, and hex parsing, so two shells cannot disagree about what a colour is.
std::vector<uint32_t> palette();
bool parseHexRgb(const std::string &text, uint32_t &outRgb);
std::string formatHexRgb(uint32_t rgb);

// Font families the engine can lay text out in, by index — the shell lists what it is told.
uint32_t fontFamilyCount();
std::string fontFamilyName(uint32_t index);
uint32_t fontFamilyStyles(uint32_t index);

// Fitting a document to a viewport, and the desk pattern behind it. The canvas skeleton needs
// the first while a project is still loading, which is the one moment there is no engine to
// ask — that is why these take plain numbers.
bool fitSize(float viewportWidth, float viewportHeight, float docWidth, float docHeight,
             float &outWidth, float &outHeight);
bool fitCamera(float viewportWidth, float viewportHeight, float docWidth, float docHeight,
               float &outZoom, float &outPanX, float &outPanY);
CalmDeskMetrics deskMetrics();

// Ruler ticks for a camera the engine does not hold yet — the same maths the engine uses on
// its own rulers, exposed for the skeleton.
std::vector<CalmRulerTick> rulerTicksX(float zoom, float pan, float viewportExtent,
                                       size_t cap = 256);
std::vector<CalmRulerTick> rulerTicksY(float zoom, float pan, float viewportExtent,
                                       size_t cap = 256);

// Decoding an image the shell only has bytes for, with no project open — the landing screen's
// paste path. Returns false when the bytes are not an image the engine can read.
bool decodeImage(const uint8_t *bytes, size_t len, std::vector<uint8_t> &outRgba, uint32_t &outW,
                 uint32_t &outH);

}  // namespace calumma::limits
