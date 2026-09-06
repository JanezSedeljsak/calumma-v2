#pragma once

#include "engine/Engine.hpp"

#include <QAbstractListModel>

namespace calumma {

// The layer stack, top row first — the reverse of `Document::layers`' bottom-to-top order,
// because that is how every layer panel (this one included) draws it. Row 0 is always the
// *last* engine index, so `engineIndex(row)` is the one place that arithmetic happens.
//
// This model holds no layer data of its own between calls: `refresh()` re-reads the engine and
// resets the model. A board this small (tens of layers, not thousands) does not need anything
// finer-grained, and re-reading is what keeps this model from ever disagreeing with the engine
// about what a layer is — the same reasoning `AppState` follows for its own knobs.
class LayerListModel : public QAbstractListModel {
    Q_OBJECT
    // See the identical note on `ProjectListModel`: `rowCount()` is not `Q_INVOKABLE`.
    Q_PROPERTY(int count READ count NOTIFY countChanged)

public:
    enum Role {
        NameRole = Qt::UserRole + 1,
        VisibleRole,
        LockedRole,
        IsPaperRole,
        OpacityRole,
        BlendModeRole,
        EngineIndexRole,
    };
    Q_ENUM(Role)

    explicit LayerListModel(Engine &engine, QObject *parent = nullptr);

    int rowCount(const QModelIndex &parent = QModelIndex()) const override;
    QVariant data(const QModelIndex &index, int role) const override;
    QHash<int, QByteArray> roleNames() const override;

    // Re-reads every layer from the engine. Called after any action that could have changed
    // the stack — add, remove, merge, undo, redo, a project switch — rather than modelled
    // incrementally, since the engine is the only source of truth for "how many layers."
    void refresh();

    uint32_t engineIndex(int row) const;
    int count() const noexcept { return static_cast<int>(m_count); }

signals:
    void countChanged();

private:
    Engine &m_engine;
    uint32_t m_count = 0;
};

}  // namespace calumma
