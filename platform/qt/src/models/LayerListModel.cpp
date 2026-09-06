#include "LayerListModel.hpp"

namespace calumma {

LayerListModel::LayerListModel(Engine &engine, QObject *parent)
    : QAbstractListModel(parent), m_engine(engine) {}

uint32_t LayerListModel::engineIndex(int row) const {
    // Row 0 is the top of the stack, which is the last engine index.
    return m_count == 0 ? 0 : m_count - 1 - static_cast<uint32_t>(row);
}

int LayerListModel::rowCount(const QModelIndex &parent) const {
    return parent.isValid() ? 0 : static_cast<int>(m_count);
}

QVariant LayerListModel::data(const QModelIndex &index, int role) const {
    if (!index.isValid() || index.row() < 0 || static_cast<uint32_t>(index.row()) >= m_count) {
        return {};
    }
    const uint32_t i = engineIndex(index.row());
    switch (role) {
    case NameRole:
        return QString::fromStdString(m_engine.layerName(i));
    case VisibleRole:
        return m_engine.layerVisible(i);
    case LockedRole:
        return m_engine.layerLocked(i);
    case IsPaperRole:
        return m_engine.layerIsPaper(i);
    case OpacityRole:
        return m_engine.layerOpacity(i);
    case BlendModeRole:
        return m_engine.layerBlendMode(i);
    case EngineIndexRole:
        return i;
    default:
        return {};
    }
}

QHash<int, QByteArray> LayerListModel::roleNames() const {
    return {
        {NameRole, "name"},
        // Not "visible" or "locked" as bare role names: every QML `Item` (which a delegate
        // always is, directly or through Rectangle/etc.) already has its own `visible` property,
        // and a `required property bool visible` in the delegate would collide with it rather
        // than bind to this role.
        {VisibleRole, "isVisible"},
        {LockedRole, "isLocked"},
        {IsPaperRole, "isPaper"},
        {OpacityRole, "opacity"},
        {BlendModeRole, "blendMode"},
        {EngineIndexRole, "engineIndex"},
    };
}

void LayerListModel::refresh() {
    beginResetModel();
    m_count = m_engine.state().layer_count;
    endResetModel();
    emit countChanged();
}

}  // namespace calumma
