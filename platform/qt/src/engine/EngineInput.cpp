#include "Engine.hpp"

// Pointer, pan and zoom. Coordinates arrive in the shell's own point space and go straight
// through: which document pixel that is, whether a drag is a stroke or a camera move, and how
// far a scroll should pan are all the engine's answers, not this file's.

namespace calumma {

bool Engine::pointerDown(float x, float y) {
    return call([&](CalmEngine *e) { return calm_engine_pointer_down(e, x, y); });
}

bool Engine::pointerMove(float x, float y) {
    return call([&](CalmEngine *e) { return calm_engine_pointer_move(e, x, y); });
}

bool Engine::pointerUp(float x, float y) {
    return call([&](CalmEngine *e) { return calm_engine_pointer_up(e, x, y); });
}

bool Engine::pan(float dx, float dy) {
    return call([&](CalmEngine *e) { return calm_engine_pan(e, dx, dy); });
}

bool Engine::panScroll(float dx, float dy, bool precise) {
    return call([&](CalmEngine *e) { return calm_engine_pan_scroll(e, dx, dy, precise ? 1 : 0); });
}

bool Engine::zoom(float x, float y, float factor) {
    return call([&](CalmEngine *e) { return calm_engine_zoom(e, x, y, factor); });
}

bool Engine::zoomScroll(float x, float y, float delta, bool precise) {
    return call(
        [&](CalmEngine *e) { return calm_engine_zoom_scroll(e, x, y, delta, precise ? 1 : 0); });
}

bool Engine::setShift(bool held) {
    return call([&](CalmEngine *e) { return calm_engine_set_shift(e, held ? 1 : 0); });
}

bool Engine::setPointerHover(float x, float y) {
    return call([&](CalmEngine *e) { return calm_engine_set_pointer_hover(e, x, y); });
}

bool Engine::clearPointerHover() {
    return call([](CalmEngine *e) { return calm_engine_clear_pointer_hover(e); });
}

bool Engine::setAlt(bool held) {
    return call([&](CalmEngine *e) { return calm_engine_set_alt(e, held ? 1 : 0); });
}

// Whether the brush ring belongs on screen at all — a tool-and-state question the cursor asks
// rather than answers.
bool Engine::brushRingVisible() const {
    return query([](CalmEngine *e) { return calm_engine_brush_ring_visible(e); }, 0) > 0;
}

bool Engine::setHoverLayer(int32_t index) {
    return call([&](CalmEngine *e) { return calm_engine_set_hover_layer(e, index); });
}

// Arrow keys with something picked up: the engine says whether they moved anything, so the
// shell knows if the keystroke was spent here or should fall through to a shortcut.
bool Engine::nudgeMoveTarget(float stepsX, float stepsY) {
    return query([&](CalmEngine *e) { return calm_engine_nudge_move_target(e, stepsX, stepsY); },
                 0) > 0;
}

}  // namespace calumma
