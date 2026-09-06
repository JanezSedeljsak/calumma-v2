#include "Engine.hpp"

// Why a tool is unavailable on the active layer, and turning a layer into one a tool can work
// on. The shell greys a button by asking and shows the reason it is given; it never works out
// for itself that, say, a text layer cannot take the blur brush.

namespace calumma {

uint32_t Engine::toolBlock(uint32_t tool) const {
    return query([&](CalmEngine *e) { return calm_engine_tool_block(e, tool); }, 0u);
}

std::vector<uint32_t> Engine::toolBlocks(uint32_t toolCount) const {
    if (!m_engine || toolCount == 0) {
        return {};
    }
    std::vector<uint32_t> out(toolCount, 0u);
    const uint32_t written = calm_engine_tool_blocks(m_engine.get(), out.data(), toolCount);
    out.resize(written < toolCount ? written : toolCount);
    return out;
}

bool Engine::takeToolBlockNotice(uint32_t &outTool) {
    return call([&](CalmEngine *e) { return calm_engine_take_tool_block_notice(e, &outTool); });
}

bool Engine::vectorModeLocked() const {
    return query([](CalmEngine *e) { return calm_engine_vector_mode_locked(e); }, 0) > 0;
}

bool Engine::layerIsRasterizable(uint32_t index) const {
    return query([&](CalmEngine *e) { return calm_engine_layer_is_rasterizable(e, index); }, 0) > 0;
}

bool Engine::rasterizeLayer(uint32_t index) {
    return call([&](CalmEngine *e) { return calm_engine_rasterize_layer(e, index); });
}

}  // namespace calumma
