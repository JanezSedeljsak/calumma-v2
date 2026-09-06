#include "Engine.hpp"

#include "ImageList.hpp"

// Selection and the clipboard. Copy and cut hand back bytes plus the kind they are, and the
// shell's only job is to put that on the system pasteboard under the matching type — it never
// decides what a copy contains.

namespace calumma {
namespace {

Clipping takeClipping(bool ok, uint8_t *data, size_t len, uint32_t kind) {
    Clipping clipping;
    if (!ok) {
        return clipping;
    }
    clipping.bytes = Bytes(data, len);
    clipping.kind = kind;
    return clipping;
}

}  // namespace

bool Engine::hasSelection() const {
    return query([](CalmEngine *e) { return calm_engine_has_selection(e); }, 0) > 0;
}

bool Engine::selectAll() {
    return call([](CalmEngine *e) { return calm_engine_select_all(e); });
}

bool Engine::invertSelection() {
    return call([](CalmEngine *e) { return calm_engine_invert_selection(e); });
}

bool Engine::deselect() {
    return call([](CalmEngine *e) { return calm_engine_deselect(e); });
}

bool Engine::selectionClearPixels() {
    return call([](CalmEngine *e) { return calm_engine_selection_clear_pixels(e); });
}

Image Engine::selectionRgba() const {
    return ownedImage([](CalmEngine *e, uint8_t **rgba, uint32_t *w, uint32_t *h) {
        return calm_engine_selection_rgba(e, rgba, w, h);
    });
}

Clipping Engine::copy() {
    uint8_t *data = nullptr;
    size_t len = 0;
    uint32_t kind = 0;
    const bool ok =
        call([&](CalmEngine *e) { return calm_engine_copy(e, &data, &len, &kind); });
    return takeClipping(ok, data, len, kind);
}

Clipping Engine::cut() {
    uint8_t *data = nullptr;
    size_t len = 0;
    uint32_t kind = 0;
    const bool ok = call([&](CalmEngine *e) { return calm_engine_cut(e, &data, &len, &kind); });
    return takeClipping(ok, data, len, kind);
}

Clipping Engine::copyLayer(uint32_t layerIndex) {
    uint8_t *data = nullptr;
    size_t len = 0;
    uint32_t kind = 0;
    const bool ok = call([&](CalmEngine *e) {
        return calm_engine_copy_layer(e, layerIndex, &data, &len, &kind);
    });
    return takeClipping(ok, data, len, kind);
}

bool Engine::pasteImage(const uint8_t *premultipliedRgba, size_t len, uint32_t width,
                        uint32_t height, uint32_t &outOutcome) {
    return call([&](CalmEngine *e) {
        return calm_engine_paste_image(e, premultipliedRgba, len, width, height, &outOutcome);
    });
}

bool Engine::pasteEncoded(const uint8_t *bytes, size_t len, uint32_t &outOutcome) {
    return call(
        [&](CalmEngine *e) { return calm_engine_paste_encoded(e, bytes, len, &outOutcome); });
}

bool Engine::pasteImages(const std::vector<PasteImage> &images, uint32_t &outCount,
                         uint32_t &outOutcome) {
    if (images.empty()) {
        return false;
    }
    const std::vector<CalmPasteImage> raw = detail::toPasteImages(images);
    return call([&](CalmEngine *e) {
        return calm_engine_paste_images(e, raw.data(), raw.size(), &outCount, &outOutcome);
    });
}

bool Engine::pasteEncodedImages(const std::vector<EncodedImage> &images, uint32_t &outCount,
                                uint32_t &outOutcome) {
    if (images.empty()) {
        return false;
    }
    const std::vector<CalmEncodedImage> raw = detail::toEncodedImages(images);
    return call([&](CalmEngine *e) {
        return calm_engine_paste_encoded_images(e, raw.data(), raw.size(), &outCount, &outOutcome);
    });
}

}  // namespace calumma
