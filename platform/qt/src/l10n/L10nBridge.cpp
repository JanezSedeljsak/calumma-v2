#include "L10nBridge.hpp"

#include <QCoreApplication>
#include <QDir>
#include <QFile>

namespace calumma {
namespace {

// Qt resource paths (`:/...`) are not real files, so `Catalog::loadFile`'s `std::ifstream`
// cannot see them — this reads the bytes with `QFile`, which understands both the resource
// system and the plain filesystem, and hands them to `Catalog::loadJson`.
bool loadFromQtPath(l10n::Catalog &catalog, const QString &path) {
    QFile file(path);
    if (!file.open(QIODevice::ReadOnly)) {
        return false;
    }
    return catalog.loadJson(file.readAll().toStdString());
}

}  // namespace

void L10nBridge::setLanguage(const QString &language) {
    l10n::Catalog next;
    // The bundled resource is what a packaged build actually ships; the two filesystem
    // fallbacks are dev-tree convenience, the same spirit as `SvgIconStore`'s walk-up-from-
    // the-executable on the Swift side, for running straight out of the CMake build directory
    // before `resources.qrc` is wired into a real install.
    const bool loaded =
        loadFromQtPath(next, QStringLiteral(":/translations/%1.json").arg(language)) ||
        loadFromQtPath(next, QCoreApplication::applicationDirPath() +
                                 QStringLiteral("/translations/%1.json").arg(language)) ||
        loadFromQtPath(next, QDir(QCoreApplication::applicationDirPath())
                                 .filePath(QStringLiteral("../../../translations/%1.json")
                                              .arg(language)));
    if (!loaded) {
        return;
    }
    m_catalog = std::move(next);
    m_language = language;
    emit languageChanged();
}

QString L10nBridge::t(const QString &key) const {
    return QString::fromStdString(m_catalog(key.toStdString()));
}

QString L10nBridge::t1(const QString &key, const QString &arg0) const {
    return QString::fromStdString(
        m_catalog.format(key.toStdString(), {arg0.toStdString()}));
}

}  // namespace calumma
