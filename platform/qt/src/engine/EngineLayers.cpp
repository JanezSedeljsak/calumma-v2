#include "Engine.hpp"

// The layer stack. Every predicate here — paper, locked, visible — is asked of the engine
// rather than tracked alongside it, so the shell cannot drift out of agreement with the
// document about what a layer is.

namespace calumma {

bool Engine::addLayer() {
    return call([](CalmEngine *e) { return calm_engine_add_layer(e); });
}

bool Engine::removeLayer(uint32_t index) {
    return call([&](CalmEngine *e) { return calm_engine_remove_layer(e, index); });
}

bool Engine::duplicateLayer(uint32_t index) {
    return call([&](CalmEngine *e) { return calm_engine_duplicate_layer(e, index); });
}

bool Engine::mergeLayerDown(uint32_t index) {
    return call([&](CalmEngine *e) { return calm_engine_merge_layer_down(e, index); });
}

bool Engine::layerCanMergeDown(uint32_t index) const {
    return query([&](CalmEngine *e) { return calm_engine_layer_can_merge_down(e, index); }, 0) > 0;
}

bool Engine::clipLayerDown(uint32_t index) {
    return call([&](CalmEngine *e) { return calm_engine_clip_layer_down(e, index); });
}

bool Engine::layerCanClipDown(uint32_t index) const {
    return query([&](CalmEngine *e) { return calm_engine_layer_can_clip_down(e, index); }, 0) > 0;
}

bool Engine::moveLayerUp(uint32_t index) {
    return call([&](CalmEngine *e) { return calm_engine_move_layer_up(e, index); });
}

bool Engine::moveLayerDown(uint32_t index) {
    return call([&](CalmEngine *e) { return calm_engine_move_layer_down(e, index); });
}

bool Engine::clearLayer() {
    return call([](CalmEngine *e) { return calm_engine_clear_layer(e); });
}

bool Engine::moveLayerRow(uint32_t fromRow, uint32_t toRow) {
    return call([&](CalmEngine *e) { return calm_engine_move_layer_row(e, fromRow, toRow); });
}

bool Engine::setActiveLayer(uint32_t index) {
    return call([&](CalmEngine *e) { return calm_engine_set_active_layer(e, index); });
}

bool Engine::setLayerSelection(const std::vector<uint32_t> &indices) {
    return call([&](CalmEngine *e) {
        return calm_engine_set_layer_selection(e, indices.data(), indices.size());
    });
}

bool Engine::alignLayers(const std::vector<uint32_t> &indices, uint32_t edge) {
    return call([&](CalmEngine *e) {
        return calm_engine_align_layers(e, indices.data(), indices.size(), edge);
    });
}

bool Engine::distributeLayers(const std::vector<uint32_t> &indices, uint32_t axis) {
    return call([&](CalmEngine *e) {
        return calm_engine_distribute_layers(e, indices.data(), indices.size(), axis);
    });
}

std::string Engine::layerName(uint32_t index) const {
    if (!m_engine) {
        return {};
    }
    return ownedString(calm_engine_layer_name(m_engine.get(), index));
}

bool Engine::setLayerName(uint32_t index, const std::string &name) {
    return call([&](CalmEngine *e) { return calm_engine_set_layer_name(e, index, name.c_str()); });
}

bool Engine::layerVisible(uint32_t index) const {
    return query([&](CalmEngine *e) { return calm_engine_layer_visible(e, index); }, 0) > 0;
}

bool Engine::setLayerVisible(uint32_t index, bool visible) {
    return call(
        [&](CalmEngine *e) { return calm_engine_set_layer_visible(e, index, visible ? 1 : 0); });
}

bool Engine::layerLocked(uint32_t index) const {
    return query([&](CalmEngine *e) { return calm_engine_layer_locked(e, index); }, 0) > 0;
}

bool Engine::setLayerLocked(uint32_t index, bool locked) {
    return call(
        [&](CalmEngine *e) { return calm_engine_set_layer_locked(e, index, locked ? 1 : 0); });
}

bool Engine::layerIsPaper(uint32_t index) const {
    return query([&](CalmEngine *e) { return calm_engine_layer_is_paper(e, index); }, 0) > 0;
}

float Engine::layerOpacity(uint32_t index) const {
    return query([&](CalmEngine *e) { return calm_engine_layer_opacity(e, index); }, 0.0f);
}

bool Engine::setLayerOpacity(uint32_t index, float opacity) {
    return call([&](CalmEngine *e) { return calm_engine_set_layer_opacity(e, index, opacity); });
}

uint32_t Engine::layerBlendMode(uint32_t index) const {
    return query([&](CalmEngine *e) { return calm_engine_layer_blend_mode(e, index); }, 0u);
}

bool Engine::setLayerBlendMode(uint32_t index, uint32_t mode) {
    return call([&](CalmEngine *e) { return calm_engine_set_layer_blend_mode(e, index, mode); });
}

std::string Engine::layerId(uint32_t index) const {
    if (!m_engine) {
        return {};
    }
    return ownedString(calm_engine_layer_id(m_engine.get(), index));
}

bool Engine::layerIsText(uint32_t index) const {
    return query([&](CalmEngine *e) { return calm_engine_layer_is_text(e, index); }, 0) > 0;
}

bool Engine::layerIsVector(uint32_t index) const {
    return query([&](CalmEngine *e) { return calm_engine_layer_is_vector(e, index); }, 0) > 0;
}

uint32_t Engine::layerItemCount(uint32_t index) const {
    return query([&](CalmEngine *e) { return calm_engine_layer_item_count(e, index); }, 0u);
}

CalmAdjustments Engine::layerAdjustments(uint32_t index) const {
    CalmAdjustments out{};
    call([&](CalmEngine *e) { return calm_engine_layer_adjustments(e, index, &out); });
    return out;
}

bool Engine::setLayerAdjustments(uint32_t index, const CalmAdjustments &adjustments) {
    return call([&](CalmEngine *e) {
        return calm_engine_set_layer_adjustments(e, index, adjustments.brightness,
                                                 adjustments.contrast, adjustments.vibrance,
                                                 adjustments.saturation, adjustments.levels_gamma);
    });
}

bool Engine::nudgeLayerAdjustment(uint32_t index, uint32_t kind, float steps) {
    return call(
        [&](CalmEngine *e) { return calm_engine_nudge_layer_adjustment(e, index, kind, steps); });
}

bool Engine::layerBounds(uint32_t index, Bounds &out) const {
    CalmLayerBounds raw{};
    if (!call([&](CalmEngine *e) { return calm_engine_layer_bounds(e, index, &raw); })) {
        return false;
    }
    out = Bounds{raw.x, raw.y, raw.width, raw.height};
    return true;
}

bool Engine::setLayerBounds(uint32_t index, const Bounds &bounds) {
    return call([&](CalmEngine *e) {
        return calm_engine_set_layer_bounds(e, index, bounds.x, bounds.y, bounds.width,
                                            bounds.height);
    });
}

bool Engine::resetLayerTransform(uint32_t index) {
    return call([&](CalmEngine *e) { return calm_engine_reset_layer_transform(e, index); });
}

uint64_t Engine::layerPreviewRevision(uint32_t index) const {
    return query([&](CalmEngine *e) { return calm_engine_layer_preview_revision(e, index); },
                 uint64_t{0});
}

}  // namespace calumma
