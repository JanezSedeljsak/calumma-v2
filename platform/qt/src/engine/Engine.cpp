#include "Engine.hpp"

// Lifetime, the surface, the frame, the camera and history — what a board needs to exist and
// be looked at. Everything else is in a sibling file.

namespace calumma {

Engine::Engine(const std::string &dbPath)
    : m_engine(calm_engine_new(dbPath.empty() ? nullptr : dbPath.c_str())) {}

std::string Engine::ownedString(char *s) const {
    const EngineString owned(s);
    return owned ? std::string(owned.get()) : std::string();
}

bool Engine::attach(const CalmNativeSurface &surface, uint32_t width, uint32_t height,
                    float scale) {
    return call([&](CalmEngine *e) {
        return calm_engine_attach_native_surface(e, &surface, width, height, scale);
    });
}

bool Engine::resize(uint32_t width, uint32_t height, float scale) {
    return call([&](CalmEngine *e) { return calm_engine_resize(e, width, height, scale); });
}

bool Engine::resizeDocument(uint32_t width, uint32_t height) {
    return call([&](CalmEngine *e) { return calm_engine_resize_document(e, width, height); });
}

bool Engine::render() {
    return call([](CalmEngine *e) { return calm_engine_render(e); });
}

uint32_t Engine::frameHint() const {
    return query([](CalmEngine *e) { return calm_engine_frame_hint(e); }, 0u);
}

CalmState Engine::state() const {
    CalmState out{};
    call([&](CalmEngine *e) { return calm_engine_state(e, &out); });
    return out;
}

Viewport Engine::viewport() const {
    Viewport out;
    call([&](CalmEngine *e) { return calm_engine_viewport(e, &out.width, &out.height); });
    return out;
}

bool Engine::fit() {
    return call([](CalmEngine *e) { return calm_engine_fit(e); });
}

bool Engine::setZoom(float zoom) {
    return call([&](CalmEngine *e) { return calm_engine_set_zoom(e, zoom); });
}

bool Engine::stepZoom(bool zoomIn) {
    return call([&](CalmEngine *e) { return calm_engine_step_zoom(e, zoomIn ? 1 : 0); });
}

bool Engine::setZoomUnit(float unit) {
    return call([&](CalmEngine *e) { return calm_engine_set_zoom_unit(e, unit); });
}

bool Engine::endCameraMotion() {
    return call([](CalmEngine *e) { return calm_engine_end_camera_motion(e); });
}

bool Engine::undo() {
    return call([](CalmEngine *e) { return calm_engine_undo(e); });
}

bool Engine::redo() {
    return call([](CalmEngine *e) { return calm_engine_redo(e); });
}

bool Engine::setBoardColors(uint32_t desk, uint32_t grid, uint32_t paperBorder) {
    return call(
        [&](CalmEngine *e) { return calm_engine_set_board_colors(e, desk, grid, paperBorder); });
}

bool Engine::setDark(bool dark) {
    return call([&](CalmEngine *e) { return calm_engine_set_dark(e, dark ? 1 : 0); });
}

bool Engine::setMemoryPressure(uint32_t level) {
    return call([&](CalmEngine *e) { return calm_engine_set_memory_pressure(e, level); });
}

CalmMemory Engine::memory() const {
    CalmMemory out{};
    call([&](CalmEngine *e) { return calm_engine_memory(e, &out); });
    return out;
}

}  // namespace calumma
