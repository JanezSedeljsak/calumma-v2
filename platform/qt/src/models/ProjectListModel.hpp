#pragma once

#include "engine/Engine.hpp"

#include <QAbstractListModel>
#include <vector>

namespace calumma {

// A list of projects — recents on the landing screen, or the open tab row, depending on what
// `refresh` is told to load. Both are "some of the store's projects, in some order," so one
// model covers both rather than two nearly-identical ones.
class ProjectListModel : public QAbstractListModel {
    Q_OBJECT
    // `rowCount()` is a plain virtual on `QAbstractItemModel`, not `Q_INVOKABLE` — QML cannot
    // call it, bare or otherwise. Anything outside a `ListView`/`Repeater` that needs "is this
    // empty" (a Recents header, an empty-state message) needs this instead.
    Q_PROPERTY(int count READ count NOTIFY countChanged)

public:
    enum Role {
        IdRole = Qt::UserRole + 1,
        NameRole,
        WidthRole,
        HeightRole,
        OpenedAtRole,
        AccentRole,
    };
    Q_ENUM(Role)

    explicit ProjectListModel(QObject *parent = nullptr);

    int rowCount(const QModelIndex &parent = QModelIndex()) const override;
    QVariant data(const QModelIndex &index, int role) const override;
    QHash<int, QByteArray> roleNames() const override;

    void setItems(std::vector<ProjectInfo> items);
    const std::vector<ProjectInfo> &items() const noexcept { return m_items; }
    int count() const noexcept { return static_cast<int>(m_items.size()); }

signals:
    void countChanged();

private:
    std::vector<ProjectInfo> m_items;
};

}  // namespace calumma
