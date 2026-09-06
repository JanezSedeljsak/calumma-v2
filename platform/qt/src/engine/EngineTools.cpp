#include "Engine.hpp"

// The tool knobs the shell is allowed to hold — active tool, brush, colours, per-tool options.
// Which of them a given tool actually takes is `calm_tool_takes_*`, not something restated
// here.

namespace calumma {

bool Engine::setTool(uint32_t tool) {
    return call([&](CalmEngine *e) { return calm_engine_set_tool(e, tool); });
}

bool Engine::setColor(Rgba color) {
    return call(
        [&](CalmEngine *e) { return calm_engine_set_color(e, color.r, color.g, color.b, color.a); });
}

bool Engine::setStrokeColor(Rgba color) {
    return call([&](CalmEngine *e) {
        return calm_engine_set_stroke_color(e, color.r, color.g, color.b, color.a);
    });
}

bool Engine::setShapeFillColor(Rgba color) {
    return call([&](CalmEngine *e) {
        return calm_engine_set_shape_fill_color(e, color.r, color.g, color.b, color.a);
    });
}

bool Engine::setSelectColor(Rgba color) {
    return call([&](CalmEngine *e) {
        return calm_engine_set_select_color(e, color.r, color.g, color.b, color.a);
    });
}

bool Engine::selectColor(uint32_t &outRgba) const {
    return call([&](CalmEngine *e) { return calm_engine_get_select_color(e, &outRgba); });
}

bool Engine::setBrush(float size) {
    return call([&](CalmEngine *e) { return calm_engine_set_brush(e, size); });
}

bool Engine::setBrushKind(uint32_t brush) {
    return call([&](CalmEngine *e) { return calm_engine_set_brush_kind(e, brush); });
}

bool Engine::setInkOpacity(float opacity) {
    return call([&](CalmEngine *e) { return calm_engine_set_ink_opacity(e, opacity); });
}

bool Engine::setEraserHardness(float hardness) {
    return call([&](CalmEngine *e) { return calm_engine_set_eraser_hardness(e, hardness); });
}

bool Engine::setBlurStrength(float strength) {
    return call([&](CalmEngine *e) { return calm_engine_set_blur_strength(e, strength); });
}

bool Engine::setTolerance(uint8_t tolerance) {
    return call([&](CalmEngine *e) { return calm_engine_set_tolerance(e, tolerance); });
}

bool Engine::setEyedropperRadius(uint32_t radius) {
    return call([&](CalmEngine *e) { return calm_engine_set_eyedropper_radius(e, radius); });
}

bool Engine::setCloneAligned(bool aligned) {
    return call([&](CalmEngine *e) { return calm_engine_set_clone_aligned(e, aligned ? 1 : 0); });
}

bool Engine::setFill(bool fill) {
    return call([&](CalmEngine *e) { return calm_engine_set_fill(e, fill ? 1 : 0); });
}

bool Engine::setStroke(bool stroke) {
    return call([&](CalmEngine *e) { return calm_engine_set_stroke(e, stroke ? 1 : 0); });
}

bool Engine::sampleColor(float x, float y, uint32_t &outRgba) const {
    return call([&](CalmEngine *e) { return calm_engine_sample_color(e, x, y, &outRgba); });
}

bool Engine::pickColor(float x, float y, uint32_t &outRgba) const {
    return call([&](CalmEngine *e) { return calm_engine_pick_color(e, x, y, &outRgba); });
}

}  // namespace calumma
