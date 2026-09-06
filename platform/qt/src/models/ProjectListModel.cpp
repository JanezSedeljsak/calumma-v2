#include "ProjectListModel.hpp"

namespace calumma {

ProjectListModel::ProjectListModel(QObject *parent) : QAbstractListModel(parent) {}

int ProjectListModel::rowCount(const QModelIndex &parent) const {
    return parent.isValid() ? 0 : static_cast<int>(m_items.size());
}

QVariant ProjectListModel::data(const QModelIndex &index, int role) const {
    if (!index.isValid() || index.row() < 0 ||
        static_cast<size_t>(index.row()) >= m_items.size()) {
        return {};
    }
    const ProjectInfo &item = m_items[static_cast<size_t>(index.row())];
    switch (role) {
    case IdRole:
        return QString::fromStdString(item.id);
    case NameRole:
        return QString::fromStdString(item.name);
    case WidthRole:
        return item.width;
    case HeightRole:
        return item.height;
    case OpenedAtRole:
        return static_cast<qlonglong>(item.openedAt);
    case AccentRole:
        return item.accent;
    default:
        return {};
    }
}

QHash<int, QByteArray> ProjectListModel::roleNames() const {
    return {
        {IdRole, "projectId"},
        {NameRole, "name"},
        // Not "width"/"height": every QML delegate is an `Item` (directly or through
        // Rectangle), which already has both — the same collision `LayerListModel` avoids by
        // not using "visible"/"locked".
        {WidthRole, "projectWidth"},
        {HeightRole, "projectHeight"},
        {OpenedAtRole, "openedAt"},
        {AccentRole, "accent"},
    };
}

void ProjectListModel::setItems(std::vector<ProjectInfo> items) {
    beginResetModel();
    m_items = std::move(items);
    endResetModel();
    emit countChanged();
}

}  // namespace calumma
