#include "Engine.hpp"

// Smart Tools. Some run in the engine (Upscale, Seam Carve, Smart Matte); Remove Background and
// the rest are platform ops, and this shell installs no vtable for them — Vision is macOS. The
// entry stays visible and greyed on Windows and Linux, because `opAvailable` answers false
// rather than the shell hiding what the other platform has.

namespace calumma {

bool Engine::installPlatformOps(const CalmPlatformOps &ops) {
    return call([&](CalmEngine *e) { return calm_engine_install_platform_ops(e, &ops); });
}

bool Engine::opAvailable(uint32_t kind) const {
    return query([&](CalmEngine *e) { return calm_engine_op_available(e, kind); }, false);
}

bool Engine::runOp(uint32_t kind, uint32_t layerIndex) {
    return call([&](CalmEngine *e) { return calm_engine_run_op(e, kind, layerIndex); });
}

bool Engine::upscaleLayer(uint32_t layerIndex, float scale) {
    return call([&](CalmEngine *e) { return calm_engine_upscale_layer(e, layerIndex, scale); });
}

bool Engine::smartMatte(uint32_t layerIndex) {
    return call([&](CalmEngine *e) { return calm_engine_smart_matte(e, layerIndex); });
}

bool Engine::seamCarveLayer(uint32_t layerIndex, uint32_t width, uint32_t height) {
    return call([&](CalmEngine *e) {
        return calm_engine_seam_carve_layer(e, layerIndex, width, height);
    });
}

}  // namespace calumma
