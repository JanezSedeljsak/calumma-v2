#include "Engine.hpp"

#include "ImageList.hpp"

// The store: making, opening, listing and closing projects, and the open-tab ids the titlebar
// restores a session from. One document is resident at a time — a tab switch is a save and
// close followed by an open, which is the engine's rule, not a shell policy.

namespace calumma {

using detail::toEncodedImages;
using detail::toPasteImages;

std::string Engine::createProject(const std::string &name, uint32_t width, uint32_t height) {
    if (!m_engine) {
        return {};
    }
    return ownedString(calm_project_create(m_engine.get(), name.c_str(), width, height));
}

std::string Engine::createProjectFromImage(const std::string &name, uint32_t width,
                                           uint32_t height, const uint8_t *premultipliedRgba,
                                           size_t len) {
    if (!m_engine) {
        return {};
    }
    return ownedString(calm_project_create_from_image(m_engine.get(), name.c_str(), width, height,
                                                      premultipliedRgba, len));
}

std::string Engine::createProjectFromEncoded(const std::string &name, const uint8_t *bytes,
                                             size_t len) {
    if (!m_engine) {
        return {};
    }
    // The engine decodes. The shell reads the file and hands over what was in it — no Qt image
    // codecs anywhere in this directory.
    return ownedString(calm_project_create_from_encoded(m_engine.get(), name.c_str(), bytes, len));
}

std::string Engine::createProjectFromImages(const std::string &name,
                                            const std::vector<PasteImage> &images) {
    if (!m_engine || images.empty()) {
        return {};
    }
    const std::vector<CalmPasteImage> raw = toPasteImages(images);
    return ownedString(
        calm_project_create_from_images(m_engine.get(), name.c_str(), raw.data(), raw.size()));
}

std::string Engine::createProjectFromEncodedImages(const std::string &name,
                                                   const std::vector<EncodedImage> &images) {
    if (!m_engine || images.empty()) {
        return {};
    }
    const std::vector<CalmEncodedImage> raw = toEncodedImages(images);
    return ownedString(calm_project_create_from_encoded_images(m_engine.get(), name.c_str(),
                                                               raw.data(), raw.size()));
}

bool Engine::openProject(const std::string &id) {
    return call([&](CalmEngine *e) { return calm_project_open(e, id.c_str()); });
}

bool Engine::closeProject() {
    return call([](CalmEngine *e) { return calm_project_close(e); });
}

bool Engine::saveProject() {
    return call([](CalmEngine *e) { return calm_project_save(e); });
}

bool Engine::deleteProject(const std::string &id) {
    return call([&](CalmEngine *e) { return calm_project_delete(e, id.c_str()); });
}

bool Engine::deleteAllProjects() {
    return call([](CalmEngine *e) { return calm_project_delete_all(e); });
}

bool Engine::renameProject(const std::string &id, const std::string &name) {
    return call([&](CalmEngine *e) { return calm_project_rename(e, id.c_str(), name.c_str()); });
}

bool Engine::setProjectAccent(const std::string &id, uint32_t accent) {
    return call([&](CalmEngine *e) { return calm_project_set_accent(e, id.c_str(), accent); });
}

bool Engine::project(const std::string &id, ProjectInfo &out) const {
    CalmProjectInfo row{};
    if (!call([&](CalmEngine *e) { return calm_project_get(e, id.c_str(), &row); })) {
        return false;
    }
    // Both strings belong to this side the moment the call succeeds.
    out.id = ownedString(row.id);
    out.name = ownedString(row.name);
    out.width = row.width;
    out.height = row.height;
    out.openedAt = row.opened_at;
    out.accent = row.accent;
    return true;
}

std::vector<ProjectInfo> Engine::projects(size_t limit) const {
    std::vector<ProjectInfo> items;
    if (!m_engine || limit == 0) {
        return items;
    }
    // The engine fills in as many as fit and returns how many it wrote; both strings in each
    // row are owned by this side from that moment, whether or not the row is usable.
    std::vector<CalmProjectInfo> raw(limit, CalmProjectInfo{});
    const size_t count = calm_project_list(m_engine.get(), raw.data(), limit);
    items.reserve(count);
    for (size_t i = 0; i < count && i < raw.size(); ++i) {
        const CalmProjectInfo &row = raw[i];
        ProjectInfo info;
        info.id = ownedString(row.id);
        info.name = ownedString(row.name);
        info.width = row.width;
        info.height = row.height;
        info.openedAt = row.opened_at;
        info.accent = row.accent;
        if (!info.id.empty()) {
            items.push_back(std::move(info));
        }
    }
    return items;
}

Bytes Engine::projectThumbnail(const std::string &id) const {
    return ownedBytes([&](CalmEngine *e, uint8_t **out, size_t *len) {
        return calm_project_thumbnail(e, id.c_str(), out, len);
    });
}

std::vector<std::string> Engine::openTabs(size_t limit) const {
    std::vector<std::string> ids;
    if (!m_engine || limit == 0) {
        return ids;
    }
    std::vector<char *> raw(limit, nullptr);
    const size_t count = calm_open_project_tabs(m_engine.get(), raw.data(), limit);
    ids.reserve(count);
    for (size_t i = 0; i < count && i < raw.size(); ++i) {
        std::string id = ownedString(raw[i]);
        if (!id.empty()) {
            ids.push_back(std::move(id));
        }
    }
    return ids;
}

bool Engine::setOpenTabs(const std::vector<std::string> &ids) {
    std::vector<const char *> pointers;
    pointers.reserve(ids.size());
    for (const std::string &id : ids) {
        pointers.push_back(id.c_str());
    }
    return call([&](CalmEngine *e) {
        return calm_set_open_project_tabs(e, pointers.data(), pointers.size());
    });
}

}  // namespace calumma
