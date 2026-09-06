#pragma once

#include "Catalog.hpp"

#include <QObject>
#include <QString>

namespace calumma {

// QML cannot call `Catalog::operator()` — this is the QObject wrapper that lets `L10n.t("brand")`
// work as an expression QML can bind to, and re-evaluate when the language changes.
class L10nBridge : public QObject {
    Q_OBJECT
    Q_PROPERTY(QString language READ language WRITE setLanguage NOTIFY languageChanged)

public:
    explicit L10nBridge(QObject *parent = nullptr) : QObject(parent) {}

    const QString &language() const noexcept { return m_language; }
    // Loads `translations/<lang>.json`. Falls back to whatever was already loaded (English, on
    // first run) if the file is missing, so a bad language code cannot blank the whole UI.
    void setLanguage(const QString &language);

    Q_INVOKABLE QString t(const QString &key) const;
    Q_INVOKABLE QString t1(const QString &key, const QString &arg0) const;

signals:
    void languageChanged();

private:
    l10n::Catalog m_catalog;
    QString m_language = "en";
};

}  // namespace calumma
