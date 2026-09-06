#pragma once

#include "AppState.hpp"
#include "engine/Engine.hpp"
#include "models/LayerListModel.hpp"
#include "models/ProjectListModel.hpp"
#include "theme/ThemeBridge.hpp"

#include <QObject>
#include <QString>
#include <QTimer>

class QQuickItem;

namespace calumma {

class BoardWindow;

// The QObject a QML screen actually talks to — `AppModel`'s job, restated for a tree that
// cannot include a C++ header. Owns the `Engine`, the `AppState` knobs, and the list models;
// every mutating call re-reads `CalmState` and republishes it through `stateChanged`, which is
// the same "engine is the only truth" discipline `AppState` already follows, just wired to Qt's
// property/notify machinery instead of return values.
//
// One aggregate `stateChanged` signal covers every `CalmState`-derived property rather than one
// signal per field: QML re-reads whichever properties a binding actually used, and a project
// switch or a stroke touches most of them at once anyway.
class AppController : public QObject {
    Q_OBJECT

    Q_PROPERTY(bool showLanding READ showLanding NOTIFY navigationChanged)
    Q_PROPERTY(QString activeProjectId READ activeProjectId NOTIFY navigationChanged)
    Q_PROPERTY(calumma::ProjectListModel *recents READ recents CONSTANT)
    Q_PROPERTY(calumma::ProjectListModel *openTabs READ openTabsModel CONSTANT)
    Q_PROPERTY(calumma::LayerListModel *layers READ layers CONSTANT)

    Q_PROPERTY(quint32 docWidth READ docWidth NOTIFY stateChanged)
    Q_PROPERTY(quint32 docHeight READ docHeight NOTIFY stateChanged)
    Q_PROPERTY(qreal zoom READ zoom NOTIFY stateChanged)
    Q_PROPERTY(qreal zoomUnit READ zoomUnit NOTIFY stateChanged)
    Q_PROPERTY(bool canUndo READ canUndo NOTIFY stateChanged)
    Q_PROPERTY(bool canRedo READ canRedo NOTIFY stateChanged)
    Q_PROPERTY(bool isFit READ isFit NOTIFY stateChanged)
    Q_PROPERTY(bool transformActive READ transformActive NOTIFY stateChanged)
    Q_PROPERTY(quint32 activeLayerRow READ activeLayerRow NOTIFY stateChanged)
    Q_PROPERTY(quint32 accent READ accent NOTIFY stateChanged)

    Q_PROPERTY(quint32 tool READ tool WRITE setTool NOTIFY knobsChanged)
    Q_PROPERTY(QColor color READ color WRITE setColorKnob NOTIFY knobsChanged)
    Q_PROPERTY(QColor strokeColor READ strokeColor WRITE setStrokeColorKnob NOTIFY knobsChanged)
    Q_PROPERTY(qreal brushSize READ brushSize WRITE setBrushSize NOTIFY knobsChanged)
    Q_PROPERTY(qreal inkOpacity READ inkOpacity WRITE setInkOpacity NOTIFY knobsChanged)
    Q_PROPERTY(quint32 tolerance READ tolerance WRITE setTolerance NOTIFY knobsChanged)
    Q_PROPERTY(qreal blurStrength READ blurStrength WRITE setBlurStrength NOTIFY knobsChanged)
    Q_PROPERTY(
        qreal eraserHardness READ eraserHardness WRITE setEraserHardness NOTIFY knobsChanged)
    Q_PROPERTY(bool cloneAligned READ cloneAligned WRITE setCloneAligned NOTIFY knobsChanged)
    Q_PROPERTY(quint32 eyedropperRadius READ eyedropperRadius WRITE setEyedropperRadius NOTIFY
                  knobsChanged)
    Q_PROPERTY(bool shapeFill READ shapeFill WRITE setShapeFill NOTIFY knobsChanged)
    Q_PROPERTY(bool shapeStroke READ shapeStroke WRITE setShapeStroke NOTIFY knobsChanged)

    Q_PROPERTY(int themeMode READ themeMode WRITE setThemeMode NOTIFY themeModeChanged)

    Q_PROPERTY(QString toastText READ toastText NOTIFY toastChanged)
    Q_PROPERTY(bool toastVisible READ toastVisible NOTIFY toastChanged)
    Q_PROPERTY(bool toastIsError READ toastIsError NOTIFY toastChanged)

public:
    explicit AppController(Engine &engine, ThemeBridge &theme, QObject *parent = nullptr);

    Engine &engine() noexcept { return m_engine; }
    AppState &appState() noexcept { return m_state; }

    bool showLanding() const noexcept { return m_showLanding; }
    const QString &activeProjectId() const noexcept { return m_activeProjectId; }
    ProjectListModel *recents() noexcept { return &m_recents; }
    ProjectListModel *openTabsModel() noexcept { return &m_openTabsModel; }
    LayerListModel *layers() noexcept { return &m_layers; }

    quint32 docWidth() const noexcept { return m_cached.width; }
    quint32 docHeight() const noexcept { return m_cached.height; }
    qreal zoom() const noexcept { return m_cached.zoom; }
    qreal zoomUnit() const noexcept { return m_cached.zoom_unit; }
    bool canUndo() const noexcept { return m_cached.can_undo != 0; }
    bool canRedo() const noexcept { return m_cached.can_redo != 0; }
    bool isFit() const noexcept { return m_cached.is_fit != 0; }
    bool transformActive() const noexcept { return m_cached.transform_active != 0; }
    quint32 activeLayerRow() const;
    quint32 accent() const noexcept { return m_cached.accent; }

    quint32 tool() const noexcept { return m_state.tool(); }
    void setTool(quint32 tool);
    QColor color() const;
    void setColorKnob(const QColor &color);
    QColor strokeColor() const;
    void setStrokeColorKnob(const QColor &color);
    qreal brushSize() const noexcept { return m_state.brushSize(); }
    void setBrushSize(qreal size);
    qreal inkOpacity() const noexcept { return m_state.inkOpacity(); }
    void setInkOpacity(qreal opacity);
    quint32 tolerance() const noexcept { return m_state.tolerance(); }
    void setTolerance(quint32 tolerance);
    qreal blurStrength() const noexcept { return m_state.blurStrength(); }
    void setBlurStrength(qreal strength);
    qreal eraserHardness() const noexcept { return m_state.eraserHardness(); }
    void setEraserHardness(qreal hardness);
    bool cloneAligned() const noexcept { return m_state.cloneAligned(); }
    void setCloneAligned(bool aligned);
    quint32 eyedropperRadius() const noexcept { return m_state.eyedropperRadius(); }
    void setEyedropperRadius(quint32 radius);
    bool shapeFill() const noexcept { return m_state.shapeFill(); }
    void setShapeFill(bool fill);
    bool shapeStroke() const noexcept { return m_state.shapeStroke(); }
    void setShapeStroke(bool stroke);

    int themeMode() const noexcept { return static_cast<int>(m_state.theme()); }
    void setThemeMode(int mode);

    const QString &toastText() const noexcept { return m_toastText; }
    bool toastVisible() const noexcept { return m_toastVisible; }
    bool toastIsError() const noexcept { return m_toastIsError; }

    // Q_PROPERTY range/default readers a slider needs and should not invent for itself —
    // exposed as invokables rather than properties since QML calls them once per tool, not
    // continuously.
    Q_INVOKABLE qreal brushSizeMin() const;
    Q_INVOKABLE qreal brushSizeMax() const;
    Q_INVOKABLE qreal inkOpacityMin() const;
    Q_INVOKABLE qreal inkOpacityMax() const;
    Q_INVOKABLE qreal blurStrengthMin() const;
    Q_INVOKABLE qreal blurStrengthMax() const;
    Q_INVOKABLE qreal eraserHardnessMin() const;
    Q_INVOKABLE qreal eraserHardnessMax() const;
    Q_INVOKABLE quint32 toleranceMax() const;
    Q_INVOKABLE quint32 eyedropperRadiusMin() const;
    Q_INVOKABLE quint32 eyedropperRadiusMax() const;

    Q_INVOKABLE bool toolTakesFill(quint32 tool) const;
    Q_INVOKABLE bool toolTakesBrushSize(quint32 tool) const;
    Q_INVOKABLE bool toolTakesInkOpacity(quint32 tool) const;
    Q_INVOKABLE bool toolTakesBlurStrength(quint32 tool) const;
    Q_INVOKABLE bool toolTakesTolerance(quint32 tool) const;
    Q_INVOKABLE bool toolTakesEraserHardness(quint32 tool) const;
    Q_INVOKABLE bool toolTakesCloneAligned(quint32 tool) const;
    Q_INVOKABLE bool toolTakesEyedropperRadius(quint32 tool) const;
    Q_INVOKABLE bool toolIsShape(quint32 tool) const;
    Q_INVOKABLE bool toolIsSelection(quint32 tool) const;

    // The engine reports bytes (`CalmMemory`, tile + history + mask + vector + text + preview +
    // gpu); the shell only formats the total, the same division of labour `SettingsView.swift`'s
    // `memoryLabel` keeps.
    Q_INVOKABLE QString memoryLabel() const;

    Q_INVOKABLE QVariantList palette() const;
    Q_INVOKABLE QString formatHex(quint32 rgb) const;
    // QML cannot see a C++ reference out-parameter — invoking through the meta-object system
    // passes every argument by value, so a `bool parseHex(text, uint32 &out)` would silently
    // never write `out` back to the caller's JS variable. Returns -1 for "not a colour" instead,
    // since every valid answer is 0..0xFFFFFF.
    Q_INVOKABLE qint64 parseHex(const QString &text) const;

    // --- Navigation --------------------------------------------------------------------------
    Q_INVOKABLE void refreshRecents();
    Q_INVOKABLE void createProject(const QString &name, quint32 width, quint32 height,
                                   quint32 accentRgb);
    Q_INVOKABLE void openRecent(const QString &projectId);
    Q_INVOKABLE void switchToProject(const QString &projectId);
    Q_INVOKABLE void closeProjectTab(const QString &projectId);
    Q_INVOKABLE void deleteProject(const QString &projectId);
    Q_INVOKABLE void deleteAllRecents();
    Q_INVOKABLE void renameProject(const QString &projectId, const QString &name);

    // --- History and camera ------------------------------------------------------------------
    Q_INVOKABLE void undo();
    Q_INVOKABLE void redo();
    Q_INVOKABLE void fit();
    Q_INVOKABLE void stepZoom(bool zoomIn);
    // `zoomUnit` (the property) has no WRITE: it is read out of `CalmState`, and the engine's
    // own log curve is what turns a unit slider position back into a zoom, so a slider drags
    // through this instead of assigning the property directly.
    Q_INVOKABLE void setZoomUnit(qreal unit);
    Q_INVOKABLE void toggleTransform();

    // --- Layers --------------------------------------------------------------------------------
    Q_INVOKABLE void addLayer();
    Q_INVOKABLE void removeLayerRow(int row);
    Q_INVOKABLE void duplicateLayerRow(int row);
    Q_INVOKABLE void mergeLayerRowDown(int row);
    Q_INVOKABLE void setActiveLayerRow(int row);
    Q_INVOKABLE void setLayerRowVisible(int row, bool visible);
    Q_INVOKABLE void setLayerRowLocked(int row, bool locked);
    Q_INVOKABLE void setLayerRowName(int row, const QString &name);
    Q_INVOKABLE void setLayerRowOpacity(int row, qreal opacity);

    // --- Toasts ----------------------------------------------------------------------------
    Q_INVOKABLE void showToast(const QString &text, bool isError);
    Q_INVOKABLE void dismissToast();

    // The one entry point BoardWindow needs after any input or render call: pull `CalmState`
    // again and republish it. Public rather than a friend declaration because `BoardWindow`
    // lives in a different translation unit and a getter-only relationship is simpler than
    // threading a friend across two headers.
    void refreshState();

    // Wired once from `main.cpp`, before the QML tree is built, so `attachBoardHost` below has
    // somewhere to send the geometry it reads off the placeholder Item.
    void setBoardWindow(BoardWindow *board) { m_board = board; }

    // Called from QML (`Component.onCompleted: controller.attachBoardHost(boardHost)`) once the
    // `BoardHost` placeholder Item exists. Reparents the board under the item's window and
    // keeps it positioned under the item from then on. This is the single least-verified path
    // in the whole shell — see the plan's phase 5 notes — because there is no Qt here to run it
    // against.
    Q_INVOKABLE void attachBoardHost(QQuickItem *hostItem);

private:
    void applyTheme();
    void reloadOpenTabsModel();

    Engine &m_engine;
    AppState m_state;
    ThemeBridge &m_theme;
    // Owned by `main.cpp`, not here — `AppController` outlives nothing and this is only ever
    // read after `setBoardWindow` and before the app tears down.
    BoardWindow *m_board = nullptr;

    LayerListModel m_layers;
    ProjectListModel m_recents;
    ProjectListModel m_openTabsModel;

    bool m_showLanding = true;
    QString m_activeProjectId;
    CalmState m_cached{};

    QString m_toastText;
    bool m_toastVisible = false;
    bool m_toastIsError = false;
    QTimer m_toastTimer;

signals:
    void navigationChanged();
    void stateChanged();
    void knobsChanged();
    void themeModeChanged();
    void toastChanged();
};

}  // namespace calumma
