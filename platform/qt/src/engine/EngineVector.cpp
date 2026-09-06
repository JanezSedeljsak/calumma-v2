#include "Engine.hpp"

// Vector mode and the selected item. Which layers are vector, and whether the mode can be
// entered at all, are the engine's answers — see EngineToolGate.cpp for the lock.

namespace calumma {

bool Engine::setVectorMode(bool on) {
    return call([&](CalmEngine *e) { return calm_engine_set_vector_mode(e, on ? 1 : 0); });
}

bool Engine::vectorMode() const {
    return query([](CalmEngine *e) { return calm_engine_vector_mode(e); }, 0) > 0;
}

int Engine::selectedVectorItem() const {
    return query([](CalmEngine *e) { return calm_engine_selected_vector_item(e); }, -1);
}

bool Engine::clearVectorSelection() {
    return call([](CalmEngine *e) { return calm_engine_clear_vector_selection(e); });
}

bool Engine::deleteSelectedVectorItem() {
    return call([](CalmEngine *e) { return calm_engine_delete_selected_vector_item(e); });
}

bool Engine::nudgeSelectedVectorItem(float stepsX, float stepsY) {
    return call([&](CalmEngine *e) {
        return calm_engine_nudge_selected_vector_item(e, stepsX, stepsY);
    });
}

}  // namespace calumma
