#include "Engine.hpp"

// Rulers and guides. The ticks are the engine's — the ruler view draws what it is handed and
// works out no spacing of its own — and a guide drag is three calls that mirror the pointer.

namespace calumma {
namespace {

std::vector<CalmRulerTick> ticks(size_t cap, size_t (*fill)(CalmEngine *, CalmRulerTick *, size_t),
                                 CalmEngine *engine) {
    if (engine == nullptr || cap == 0) {
        return {};
    }
    std::vector<CalmRulerTick> out(cap, CalmRulerTick{});
    const size_t count = fill(engine, out.data(), cap);
    out.resize(count < cap ? count : cap);
    return out;
}

}  // namespace

std::vector<CalmRulerTick> Engine::rulerTicksX(size_t cap) const {
    return ticks(cap, calm_engine_ruler_ticks_x, m_engine.get());
}

std::vector<CalmRulerTick> Engine::rulerTicksY(size_t cap) const {
    return ticks(cap, calm_engine_ruler_ticks_y, m_engine.get());
}

bool Engine::guideDragFromRuler(uint8_t axis, float x, float y) {
    return call([&](CalmEngine *e) { return calm_engine_guide_drag_from_ruler(e, axis, x, y); });
}

bool Engine::guideDragUpdate(float x, float y) {
    return call([&](CalmEngine *e) { return calm_engine_guide_drag_update(e, x, y); });
}

bool Engine::guideDragEnd(float x, float y) {
    return call([&](CalmEngine *e) { return calm_engine_guide_drag_end(e, x, y); });
}

bool Engine::clearGuides() {
    return call([](CalmEngine *e) { return calm_engine_clear_guides(e); });
}

size_t Engine::guideCount() const {
    return query([](CalmEngine *e) { return calm_engine_guide_count(e); }, size_t{0});
}

int Engine::guideAxisAt(float x, float y) const {
    return query([&](CalmEngine *e) { return calm_engine_guide_axis_at(e, x, y); }, -1);
}

bool Engine::draggedGuide(DraggedGuide &out) const {
    return query(
               [&](CalmEngine *e) {
                   return calm_engine_dragged_guide(e, &out.axis, &out.position, &out.screen);
               },
               0) > 0;
}

std::vector<Guide> Engine::guides(size_t cap) const {
    if (!m_engine || cap == 0) {
        return {};
    }
    std::vector<CalmGuide> raw(cap, CalmGuide{});
    const size_t count = calm_engine_guide_list(m_engine.get(), raw.data(), cap);
    std::vector<Guide> out;
    out.reserve(count);
    for (size_t i = 0; i < count && i < raw.size(); ++i) {
        out.push_back(Guide{raw[i].axis, raw[i].position, raw[i].color});
    }
    return out;
}

bool Engine::addGuide(uint8_t axis, float position) {
    return call([&](CalmEngine *e) { return calm_engine_add_guide(e, axis, position); });
}

bool Engine::setGuidePosition(size_t index, float position) {
    return call([&](CalmEngine *e) { return calm_engine_set_guide_position(e, index, position); });
}

bool Engine::setGuideAxis(size_t index, uint8_t axis) {
    return call([&](CalmEngine *e) { return calm_engine_set_guide_axis(e, index, axis); });
}

bool Engine::setGuideColor(size_t index, uint32_t rgb) {
    return call([&](CalmEngine *e) { return calm_engine_set_guide_color(e, index, rgb); });
}

bool Engine::removeGuide(size_t index) {
    return call([&](CalmEngine *e) { return calm_engine_remove_guide(e, index); });
}

}  // namespace calumma
