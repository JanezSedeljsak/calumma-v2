#include "Engine.hpp"

// Getting pixels and files back out. Every format is encoded by the engine — PNG, JPEG, WebP,
// AVIF, HEIC, PSD, PDF, SVG — and the shell writes the bytes it is given to a file the user
// chose. No Qt image codecs, on either side of import or export.

namespace calumma {

Bytes Engine::exportImage(uint32_t format) const {
    return ownedBytes([&](CalmEngine *e, uint8_t **out, size_t *len) {
        return calm_engine_export_image(e, format, out, len);
    });
}

Bytes Engine::exportLayerImage(uint32_t layerIndex, uint32_t format) const {
    return ownedBytes([&](CalmEngine *e, uint8_t **out, size_t *len) {
        return calm_engine_export_layer_image(e, layerIndex, format, out, len);
    });
}

Bytes Engine::exportPsd() const {
    return ownedBytes([](CalmEngine *e, uint8_t **out, size_t *len) {
        return calm_engine_export_psd(e, out, len);
    });
}

Bytes Engine::exportPdf(float dpi) const {
    return ownedBytes([&](CalmEngine *e, uint8_t **out, size_t *len) {
        return calm_engine_export_pdf(e, dpi, out, len);
    });
}

std::string Engine::exportSvg() const {
    if (!m_engine) {
        return {};
    }
    return ownedString(calm_engine_export_svg(m_engine.get()));
}

std::string Engine::layerSvg(uint32_t layerIndex) const {
    if (!m_engine) {
        return {};
    }
    return ownedString(calm_engine_layer_svg(m_engine.get(), layerIndex));
}

Image Engine::compositeRgba() const {
    return ownedImage([](CalmEngine *e, uint8_t **rgba, uint32_t *w, uint32_t *h) {
        return calm_engine_composite_rgba(e, rgba, w, h);
    });
}

Image Engine::layerRgba(uint32_t layerIndex) const {
    return ownedImage([&](CalmEngine *e, uint8_t **rgba, uint32_t *w, uint32_t *h) {
        return calm_engine_layer_rgba(e, layerIndex, rgba, w, h);
    });
}

Image Engine::layerThumbnail(uint32_t layerIndex, uint32_t maxSide) const {
    return ownedImage([&](CalmEngine *e, uint8_t **rgba, uint32_t *w, uint32_t *h) {
        return calm_engine_layer_thumbnail(e, layerIndex, maxSide, rgba, w, h);
    });
}

}  // namespace calumma
