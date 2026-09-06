#pragma once

#include <initializer_list>
#include <string>
#include <string_view>
#include <unordered_map>

namespace calumma::l10n {

// The copy for one language, loaded from translations/<lang>.json — the same flat key → string
// files the macOS shell reads, because copy belongs to the product and not to a platform.
//
// A missing key answers with the key itself rather than an empty string: a screen that says
// "layerSettings" is a bug you can see and grep for, where a blank one is a bug you cannot.
class Catalog {
public:
    // False when the file is missing or is not a flat object of strings. The caller decides
    // what to do about it; a shell that cannot find its copy should say so rather than run
    // with 206 keys showing through.
    bool loadFile(const std::string &path);
    bool loadJson(std::string_view json);

    bool empty() const noexcept { return m_strings.empty(); }
    size_t size() const noexcept { return m_strings.size(); }

    std::string operator()(std::string_view key) const;
    // `{0}`, `{1}`, … substituted in order. The placeholders stay as they are in the JSON, so
    // a translator can move them around a sentence without the shell knowing.
    std::string format(std::string_view key, std::initializer_list<std::string_view> args) const;

private:
    std::unordered_map<std::string, std::string> m_strings;
};

}  // namespace calumma::l10n
