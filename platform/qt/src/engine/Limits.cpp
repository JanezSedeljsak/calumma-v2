#include "Limits.hpp"

#include "Engine.hpp"

namespace calumma::limits {
namespace {

std::string takeString(char *s) {
    const EngineString owned(s);
    return owned ? std::string(owned.get()) : std::string();
}

std::vector<CalmRulerTick> ticks(size_t (*fill)(float, float, float, CalmRulerTick *, size_t),
                                 float zoom, float pan, float viewportExtent, size_t cap) {
    if (cap == 0) {
        return {};
    }
    std::vector<CalmRulerTick> out(cap, CalmRulerTick{});
    const size_t count = fill(zoom, pan, viewportExtent, out.data(), cap);
    out.resize(count < cap ? count : cap);
    return out;
}

}  // namespace

std::vector<uint32_t> palette() {
    const uint32_t count = calm_palette_count();
    std::vector<uint32_t> colors;
    colors.reserve(count);
    for (uint32_t i = 0; i < count; ++i) {
        colors.push_back(calm_palette_color(i));
    }
    return colors;
}

bool parseHexRgb(const std::string &text, uint32_t &outRgb) {
    return calm_parse_hex_rgb(text.c_str(), &outRgb) == CalmStatusOk;
}

std::string formatHexRgb(uint32_t rgb) { return takeString(calm_format_hex_rgb(rgb)); }

uint32_t fontFamilyCount() { return calm_font_family_count(); }

std::string fontFamilyName(uint32_t index) { return takeString(calm_font_family_name(index)); }

uint32_t fontFamilyStyles(uint32_t index) { return calm_font_family_styles(index); }

bool fitSize(float viewportWidth, float viewportHeight, float docWidth, float docHeight,
             float &outWidth, float &outHeight) {
    return calm_fit_size(viewportWidth, viewportHeight, docWidth, docHeight, &outWidth,
                         &outHeight) == CalmStatusOk;
}

bool fitCamera(float viewportWidth, float viewportHeight, float docWidth, float docHeight,
               float &outZoom, float &outPanX, float &outPanY) {
    return calm_fit_camera(viewportWidth, viewportHeight, docWidth, docHeight, &outZoom, &outPanX,
                           &outPanY) == CalmStatusOk;
}

CalmDeskMetrics deskMetrics() {
    CalmDeskMetrics out{};
    calm_desk_metrics(&out);
    return out;
}

std::vector<CalmRulerTick> rulerTicksX(float zoom, float pan, float viewportExtent, size_t cap) {
    return ticks(calm_ruler_ticks_x, zoom, pan, viewportExtent, cap);
}

std::vector<CalmRulerTick> rulerTicksY(float zoom, float pan, float viewportExtent, size_t cap) {
    return ticks(calm_ruler_ticks_y, zoom, pan, viewportExtent, cap);
}

bool decodeImage(const uint8_t *bytes, size_t len, std::vector<uint8_t> &outRgba, uint32_t &outW,
                 uint32_t &outH) {
    uint8_t *data = nullptr;
    size_t decodedLen = 0;
    if (calm_image_decode(bytes, len, &data, &decodedLen, &outW, &outH) != CalmStatusOk) {
        return false;
    }
    // Copied rather than handed over: this is the one buffer a caller keeps for a while and
    // then passes back into a create-from-image call, and a vector is what that wants.
    const Bytes owned(data, decodedLen);
    outRgba.assign(owned.data(), owned.data() + owned.size());
    return true;
}

}  // namespace calumma::limits
