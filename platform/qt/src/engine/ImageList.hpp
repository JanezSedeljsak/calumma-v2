#pragma once

#include "Engine.hpp"

#include <vector>

// Turning the shell's list of dropped or pasted images into the C arrays the ABI takes. Both
// used by the create-a-project and the paste-into-a-project paths, which is why they are not
// private to either file.
//
// The rows point *into* the caller's `images` — names and bytes both — so the result must not
// outlive it. Every use is a temporary handed straight to one call, which is the only shape
// that is safe.
namespace calumma::detail {

inline std::vector<CalmPasteImage> toPasteImages(const std::vector<PasteImage> &images) {
    std::vector<CalmPasteImage> raw;
    raw.reserve(images.size());
    for (const PasteImage &image : images) {
        raw.push_back(CalmPasteImage{image.name.c_str(), image.premultipliedRgba, image.len,
                                     image.width, image.height});
    }
    return raw;
}

inline std::vector<CalmEncodedImage> toEncodedImages(const std::vector<EncodedImage> &images) {
    std::vector<CalmEncodedImage> raw;
    raw.reserve(images.size());
    for (const EncodedImage &image : images) {
        raw.push_back(CalmEncodedImage{image.name.c_str(), image.bytes, image.len});
    }
    return raw;
}

}  // namespace calumma::detail
