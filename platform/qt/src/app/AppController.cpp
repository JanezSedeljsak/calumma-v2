#include "AppController.hpp"

#include "canvas/BoardWindow.hpp"
#include "engine/Limits.hpp"

#include <QDebug>
#include <QLocale>
#include <QQuickItem>
#include <QQuickWindow>
#include <QRect>

#include <algorithm>

namespace calumma {

AppController::AppController(Engine &engine, ThemeBridge &theme, QObject *parent)
    : QObject(parent), m_engine(engine), m_state(engine), m_theme(theme), m_layers(engine) {
    m_toastTimer.setSingleShot(true);
    connect(&m_toastTimer, &QTimer::timeout, this, &AppController::dismissToast);

    applyTheme();

    m_state.restoreTabs();
    reloadOpenTabsModel();
    if (!m_state.openTabs().empty()) {
        switchToProject(QString::fromStdString(m_state.openTabs().front()));
    } else {
        refreshRecents();
    }
}

void AppController::refreshState() {
    m_cached = m_engine.state();
    emit stateChanged();
}

void AppController::reloadOpenTabsModel() {
    std::vector<ProjectInfo> items;
    for (const std::string &id : m_state.openTabs()) {
        ProjectInfo info;
        if (m_engine.project(id, info)) {
            items.push_back(std::move(info));
        }
    }
    m_openTabsModel.setItems(std::move(items));
}

quint32 AppController::activeLayerRow() const {
    if (m_cached.layer_count == 0) {
        return 0;
    }
    return m_cached.layer_count - 1 - std::min(m_cached.active_layer, m_cached.layer_count - 1);
}

void AppController::setTool(quint32 tool) {
    m_state.setTool(tool);
    emit knobsChanged();
}

QColor AppController::color() const {
    const Rgba c = m_state.color();
    return QColor(c.r, c.g, c.b, c.a);
}

void AppController::setColorKnob(const QColor &color) {
    m_state.setColor(Rgba{static_cast<uint8_t>(color.red()), static_cast<uint8_t>(color.green()),
                          static_cast<uint8_t>(color.blue()), static_cast<uint8_t>(color.alpha())});
    emit knobsChanged();
}

QColor AppController::strokeColor() const {
    const Rgba c = m_state.strokeColor();
    return QColor(c.r, c.g, c.b, c.a);
}

void AppController::setStrokeColorKnob(const QColor &color) {
    m_state.setStrokeColor(Rgba{static_cast<uint8_t>(color.red()),
                               static_cast<uint8_t>(color.green()),
                               static_cast<uint8_t>(color.blue()),
                               static_cast<uint8_t>(color.alpha())});
    emit knobsChanged();
}

void AppController::setBrushSize(qreal size) {
    m_state.setBrushSize(static_cast<float>(size));
    emit knobsChanged();
}

void AppController::setInkOpacity(qreal opacity) {
    m_state.setInkOpacity(static_cast<float>(opacity));
    emit knobsChanged();
}

void AppController::setTolerance(quint32 tolerance) {
    m_state.setTolerance(static_cast<uint8_t>(tolerance));
    emit knobsChanged();
}

void AppController::setBlurStrength(qreal strength) {
    m_state.setBlurStrength(static_cast<float>(strength));
    emit knobsChanged();
}

void AppController::setEraserHardness(qreal hardness) {
    m_state.setEraserHardness(static_cast<float>(hardness));
    emit knobsChanged();
}

void AppController::setCloneAligned(bool aligned) {
    m_state.setCloneAligned(aligned);
    emit knobsChanged();
}

void AppController::setEyedropperRadius(quint32 radius) {
    m_state.setEyedropperRadius(radius);
    emit knobsChanged();
}

void AppController::setShapeFill(bool fill) {
    m_state.setShapeFill(fill);
    emit knobsChanged();
}

void AppController::setShapeStroke(bool stroke) {
    m_state.setShapeStroke(stroke);
    emit knobsChanged();
}

void AppController::applyTheme() { m_theme.setDark(m_state.isDark()); }

void AppController::setThemeMode(int mode) {
    m_state.setTheme(static_cast<Theme>(mode));
    applyTheme();
    emit themeModeChanged();
    refreshState();
}

qreal AppController::brushSizeMin() const { return limits::brushSize().min; }
qreal AppController::brushSizeMax() const { return limits::brushSize().max; }
qreal AppController::inkOpacityMin() const { return limits::inkOpacity().min; }
qreal AppController::inkOpacityMax() const { return limits::inkOpacity().max; }
qreal AppController::blurStrengthMin() const { return limits::blurStrength().min; }
qreal AppController::blurStrengthMax() const { return limits::blurStrength().max; }
qreal AppController::eraserHardnessMin() const { return limits::eraserHardness().min; }
qreal AppController::eraserHardnessMax() const { return limits::eraserHardness().max; }
quint32 AppController::toleranceMax() const { return limits::toleranceMax(); }
quint32 AppController::eyedropperRadiusMin() const { return limits::eyedropperRadiusMin(); }
quint32 AppController::eyedropperRadiusMax() const { return limits::eyedropperRadiusMax(); }

bool AppController::toolTakesFill(quint32 tool) const { return limits::takesFill(tool); }
bool AppController::toolTakesBrushSize(quint32 tool) const { return limits::takesBrushSize(tool); }
bool AppController::toolTakesInkOpacity(quint32 tool) const {
    return limits::takesInkOpacity(tool);
}
bool AppController::toolTakesBlurStrength(quint32 tool) const {
    return limits::takesBlurStrength(tool);
}
bool AppController::toolTakesTolerance(quint32 tool) const { return limits::takesTolerance(tool); }
bool AppController::toolTakesEraserHardness(quint32 tool) const {
    return limits::takesEraserHardness(tool);
}
bool AppController::toolTakesCloneAligned(quint32 tool) const {
    return limits::takesCloneAligned(tool);
}
bool AppController::toolTakesEyedropperRadius(quint32 tool) const {
    return limits::takesEyedropperRadius(tool);
}
bool AppController::toolIsShape(quint32 tool) const { return limits::isShape(tool); }
bool AppController::toolIsSelection(quint32 tool) const { return limits::isSelection(tool); }

QString AppController::memoryLabel() const {
    const CalmMemory memory = m_engine.memory();
    const quint64 total = memory.tile_bytes + memory.history_bytes + memory.mask_bytes +
                         memory.vector_bytes + memory.text_bytes + memory.preview_bytes +
                         memory.gpu_bytes;
    return QLocale::system().formattedDataSize(static_cast<qint64>(total));
}

QVariantList AppController::palette() const {
    QVariantList out;
    for (uint32_t rgb : limits::palette()) {
        out.append(ThemeBridge::packedToColor(rgb));
    }
    return out;
}

QString AppController::formatHex(quint32 rgb) const {
    return QString::fromStdString(limits::formatHexRgb(rgb));
}

qint64 AppController::parseHex(const QString &text) const {
    uint32_t rgb = 0;
    return limits::parseHexRgb(text.toStdString(), rgb) ? static_cast<qint64>(rgb) : -1;
}

void AppController::refreshRecents() {
    m_recents.setItems(m_engine.projects());
}

void AppController::createProject(const QString &name, quint32 width, quint32 height,
                                  quint32 accentRgb) {
    const QString resolved = name.isEmpty() ? QStringLiteral("Untitled") : name;
    if (!m_activeProjectId.isEmpty()) {
        m_engine.saveProject();
        m_engine.closeProject();
    }
    const std::string id = m_engine.createProject(resolved.toStdString(), width, height);
    if (id.empty()) {
        return;
    }
    m_engine.setProjectAccent(id, accentRgb);
    m_activeProjectId = QString::fromStdString(id);
    m_showLanding = false;
    m_state.addTab(id);
    reloadOpenTabsModel();
    m_layers.refresh();
    m_engine.fit();
    refreshState();
    emit navigationChanged();
}

void AppController::openRecent(const QString &projectId) { switchToProject(projectId); }

void AppController::switchToProject(const QString &projectId) {
    if (m_activeProjectId == projectId && !m_showLanding) {
        return;
    }
    if (!m_activeProjectId.isEmpty() && m_activeProjectId != projectId) {
        m_engine.saveProject();
        m_engine.closeProject();
    }
    if (!m_engine.openProject(projectId.toStdString())) {
        return;
    }
    m_activeProjectId = projectId;
    m_showLanding = false;
    m_state.addTab(projectId.toStdString());
    reloadOpenTabsModel();
    m_layers.refresh();
    m_engine.fit();
    refreshState();
    emit navigationChanged();
}

void AppController::closeProjectTab(const QString &projectId) {
    m_state.closeTab(projectId.toStdString());
    reloadOpenTabsModel();
    if (m_activeProjectId == projectId) {
        m_engine.saveProject();
        m_engine.closeProject();
        m_activeProjectId.clear();
        if (!m_state.openTabs().empty()) {
            switchToProject(QString::fromStdString(m_state.openTabs().back()));
            return;
        }
        m_showLanding = true;
        refreshRecents();
    }
    emit navigationChanged();
}

void AppController::deleteProject(const QString &projectId) {
    m_state.closeTab(projectId.toStdString());
    reloadOpenTabsModel();
    if (m_activeProjectId == projectId) {
        m_engine.saveProject();
        m_engine.closeProject();
        m_activeProjectId.clear();
        m_showLanding = true;
    }
    m_engine.deleteProject(projectId.toStdString());
    refreshRecents();
    emit navigationChanged();
}

void AppController::deleteAllRecents() {
    m_engine.deleteAllProjects();
    m_state.clearTabs();
    reloadOpenTabsModel();
    m_activeProjectId.clear();
    m_showLanding = true;
    refreshRecents();
    emit navigationChanged();
}

void AppController::renameProject(const QString &projectId, const QString &name) {
    m_engine.renameProject(projectId.toStdString(), name.toStdString());
    refreshRecents();
    reloadOpenTabsModel();
}

void AppController::undo() {
    m_engine.undo();
    m_layers.refresh();
    refreshState();
}

void AppController::redo() {
    m_engine.redo();
    m_layers.refresh();
    refreshState();
}

void AppController::fit() {
    m_engine.fit();
    refreshState();
}

void AppController::stepZoom(bool zoomIn) {
    m_engine.stepZoom(zoomIn);
    refreshState();
}

void AppController::setZoomUnit(qreal unit) {
    m_engine.setZoomUnit(static_cast<float>(unit));
    refreshState();
}

void AppController::toggleTransform() {
    m_engine.toggleTransform();
    refreshState();
}

void AppController::addLayer() {
    m_engine.addLayer();
    m_layers.refresh();
    refreshState();
}

void AppController::removeLayerRow(int row) {
    m_engine.removeLayer(m_layers.engineIndex(row));
    m_layers.refresh();
    refreshState();
}

void AppController::duplicateLayerRow(int row) {
    m_engine.duplicateLayer(m_layers.engineIndex(row));
    m_layers.refresh();
    refreshState();
}

void AppController::mergeLayerRowDown(int row) {
    m_engine.mergeLayerDown(m_layers.engineIndex(row));
    m_layers.refresh();
    refreshState();
}

bool AppController::layerRowCanMergeDown(int row) const {
    return m_engine.layerCanMergeDown(m_layers.engineIndex(row));
}

void AppController::setActiveLayerRow(int row) {
    m_engine.setActiveLayer(m_layers.engineIndex(row));
    refreshState();
}

void AppController::setLayerRowVisible(int row, bool visible) {
    m_engine.setLayerVisible(m_layers.engineIndex(row), visible);
    m_layers.refresh();
}

void AppController::setLayerRowLocked(int row, bool locked) {
    m_engine.setLayerLocked(m_layers.engineIndex(row), locked);
    m_layers.refresh();
}

void AppController::setLayerRowName(int row, const QString &name) {
    m_engine.setLayerName(m_layers.engineIndex(row), name.toStdString());
    m_layers.refresh();
}

void AppController::setLayerRowOpacity(int row, qreal opacity) {
    m_engine.setLayerOpacity(m_layers.engineIndex(row), static_cast<float>(opacity));
    m_layers.refresh();
}

void AppController::attachBoardHost(QQuickItem *hostItem) {
    if (hostItem == nullptr || m_board == nullptr) {
        return;
    }
    QQuickWindow *host = hostItem->window();
    if (host == nullptr) {
        qWarning() << "calumma: BoardHost has no window yet — attach was called too early";
        return;
    }
    m_board->embedUnder(host);
    m_board->setVisible(true);

    // `mapToScene` is the item's position in the *window's* coordinate space, which is exactly
    // what a reparented child QWindow's own geometry wants — the placeholder never rotates or
    // scales, so this one point plus its size is the whole rectangle.
    const auto sync = [this, hostItem] {
        if (m_board == nullptr) {
            return;
        }
        const QPointF topLeft = hostItem->mapToScene(QPointF(0, 0));
        m_board->setGeometry(QRect(topLeft.toPoint(), QSize(static_cast<int>(hostItem->width()),
                                                             static_cast<int>(hostItem->height()))));
    };
    connect(hostItem, &QQuickItem::xChanged, this, sync);
    connect(hostItem, &QQuickItem::yChanged, this, sync);
    connect(hostItem, &QQuickItem::widthChanged, this, sync);
    connect(hostItem, &QQuickItem::heightChanged, this, sync);
    sync();
}

void AppController::showToast(const QString &text, bool isError) {
    m_toastText = text;
    m_toastIsError = isError;
    m_toastVisible = true;
    emit toastChanged();
    m_toastTimer.start(2500);
}

void AppController::dismissToast() {
    if (!m_toastVisible) {
        return;
    }
    m_toastVisible = false;
    emit toastChanged();
}

}  // namespace calumma
