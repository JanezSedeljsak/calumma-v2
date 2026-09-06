#include "Engine.hpp"

// Transform (⌘T on the Mac, Ctrl+T here) and the Crop tool's options. Both are engine modes
// the shell asks for and mirrors back from CalmState — `transform_active` is read, never
// tracked alongside.

namespace calumma {

bool Engine::toggleTransform() {
    return call([](CalmEngine *e) { return calm_engine_toggle_transform(e); });
}

bool Engine::enterTransform() {
    return call([](CalmEngine *e) { return calm_engine_enter_transform(e); });
}

bool Engine::exitTransform() {
    return call([](CalmEngine *e) { return calm_engine_exit_transform(e); });
}

bool Engine::setCropAspectLock(float ratio) {
    return call([&](CalmEngine *e) { return calm_engine_set_crop_aspect_lock(e, ratio); });
}

bool Engine::clearCropAspectLock() {
    return call([](CalmEngine *e) { return calm_engine_clear_crop_aspect_lock(e); });
}

bool Engine::setCropOverlayStyle(uint32_t style) {
    return call([&](CalmEngine *e) { return calm_engine_set_crop_overlay_style(e, style); });
}

bool Engine::setStraightenActive(bool active) {
    return call(
        [&](CalmEngine *e) { return calm_engine_set_straighten_active(e, active ? 1 : 0); });
}

bool Engine::commitCrop() {
    return call([](CalmEngine *e) { return calm_engine_commit_crop(e); });
}

}  // namespace calumma
