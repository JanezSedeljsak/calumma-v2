#include "BoardWindow.hpp"

#include "../app/AppController.hpp"
#include "../app/Shortcuts.hpp"
#include "../engine/Engine.hpp"
#include "../engine/Limits.hpp"
#include "NativeSurface.hpp"

#include <QDebug>
#include <QEvent>
#include <QExposeEvent>
#include <QKeyEvent>
#include <QMouseEvent>
#include <QNativeGestureEvent>
#include <QQuickWindow>
#include <QResizeEvent>
#include <QScreen>
#include <QTimer>
#include <QWheelEvent>

#include <algorithm>

namespace calumma {
namespace {

// Qt counts a notch of a wheel as 120 units of angle, and the engine counts coarse scrolling in
// lines — so one notch is one line. Precise devices report pixels in `pixelDelta`, which is the
// unit the engine's precise path already works in, so those go through untouched.
constexpr float kAngleUnitsPerLine = 120.0f;

int refreshRateOf(const QScreen *screen) {
    if (screen == nullptr) {
        return 60;
    }
    const int rate = static_cast<int>(screen->refreshRate());
    return rate > 0 ? rate : 60;
}

bool isDeleteKey(int key) { return key == Qt::Key_Delete || key == Qt::Key_Backspace; }

}  // namespace

BoardWindow::BoardWindow(Engine &engine) : m_engine(engine) {
    setTitle(QStringLiteral("Calumma"));
    resize(1280, 800);
    m_displayCeiling = refreshRateOf(screen());
}

void BoardWindow::embedUnder(QQuickWindow *host) {
    // A plain `setParent` is enough: Qt gives every `QWindow` a native handle once it is shown
    // or asked for one, and reparenting one onto a `QQuickWindow` is the documented way to put
    // a foreign surface inside a Quick scene without Quick ever compositing its pixels itself.
    // The geometry that matters — where the placeholder `Item` actually sits — is pushed in
    // separately, from `AppController::attachBoardHost`'s connections.
    setParent(host);
}

bool BoardWindow::isCameraChord(Qt::KeyboardModifiers modifiers) {
    return modifiers.testFlag(Qt::ControlModifier) || modifiers.testFlag(Qt::AltModifier);
}

bool BoardWindow::attachOnce() {
    if (m_attached) {
        return true;
    }
    if (m_attachFailed) {
        return false;
    }
    const NativeSurfaceResult native = nativeSurfaceFor(this);
    if (!native.ok) {
        m_attachFailed = true;
        qWarning() << "calumma: cannot attach the board —" << native.reason;
        return false;
    }
    // The engine takes sizes in points and the scale separately, exactly as the macOS shell
    // passes them; it does the pixel conversion itself.
    m_attached = m_engine.attach(native.surface, static_cast<uint32_t>(width()),
                                 static_cast<uint32_t>(height()),
                                 static_cast<float>(devicePixelRatio()));
    if (!m_attached) {
        m_attachFailed = true;
        qWarning() << "calumma: the engine refused the surface";
    }
    return m_attached;
}

void BoardWindow::wake() {
    m_awake = true;
    scheduleFrame();
}

void BoardWindow::scheduleFrame() {
    if (m_framePending || !isExposed()) {
        return;
    }
    m_framePending = true;
    // The engine names a *floor* it can live with, never a ceiling: zero is it saying "as often
    // as the display allows", and the ceiling is this side's business because it belongs to
    // whichever screen the window is on. A settled board on a 120Hz panel is otherwise a hundred
    // and twenty wakeups a second for a picture that is not moving — but at the ceiling,
    // requestUpdate is the better clock, because it is the one the compositor paces.
    const int hint = static_cast<int>(m_engine.frameHint());
    const int target =
        m_awake || hint == 0 ? m_displayCeiling : std::min(hint, m_displayCeiling);
    if (target >= m_displayCeiling) {
        requestUpdate();
        return;
    }
    QTimer::singleShot(1000 / std::max(1, target), this, [this] {
        m_framePending = false;
        renderFrame();
    });
}

void BoardWindow::renderFrame() {
    m_framePending = false;
    if (!isExposed() || !attachOnce()) {
        return;
    }
    m_engine.render();
    if (m_controller != nullptr) {
        m_controller->refreshState();
    }
    // One frame of grace: the engine only reports a slower hint once whatever the input started
    // has settled, so asking it again on the very next frame is what lets the board rest.
    m_awake = false;
    scheduleFrame();
}

void BoardWindow::exposeEvent(QExposeEvent *event) {
    QWindow::exposeEvent(event);
    if (isExposed()) {
        m_displayCeiling = refreshRateOf(screen());
        wake();
    }
}

void BoardWindow::resizeEvent(QResizeEvent *event) {
    QWindow::resizeEvent(event);
    if (m_attached) {
        m_engine.resize(static_cast<uint32_t>(event->size().width()),
                        static_cast<uint32_t>(event->size().height()),
                        static_cast<float>(devicePixelRatio()));
    }
    wake();
}

void BoardWindow::mousePressEvent(QMouseEvent *event) {
    wake();
    const QPointF point = event->position();
    // Space, the camera chord, or the middle button all mean the same thing: this drag moves
    // the board rather than marks it.
    if (event->button() == Qt::MiddleButton || m_spaceHeld || isCameraChord(event->modifiers())) {
        m_panning = true;
        m_lastPanPoint = point;
        return;
    }
    if (event->button() != Qt::LeftButton) {
        QWindow::mousePressEvent(event);
        return;
    }
    m_strokeActive = true;
    m_engine.pointerDown(static_cast<float>(point.x()), static_cast<float>(point.y()));
}

void BoardWindow::mouseMoveEvent(QMouseEvent *event) {
    wake();
    const QPointF point = event->position();
    if (m_panning) {
        m_engine.pan(static_cast<float>(point.x() - m_lastPanPoint.x()),
                     static_cast<float>(point.y() - m_lastPanPoint.y()));
        m_lastPanPoint = point;
        return;
    }
    if (m_strokeActive) {
        m_engine.pointerMove(static_cast<float>(point.x()), static_cast<float>(point.y()));
        return;
    }
    m_engine.setPointerHover(static_cast<float>(point.x()), static_cast<float>(point.y()));
}

void BoardWindow::mouseReleaseEvent(QMouseEvent *event) {
    wake();
    const QPointF point = event->position();
    if (m_panning) {
        m_panning = false;
        m_engine.endCameraMotion();
        return;
    }
    if (!m_strokeActive) {
        QWindow::mouseReleaseEvent(event);
        return;
    }
    m_strokeActive = false;
    m_engine.pointerUp(static_cast<float>(point.x()), static_cast<float>(point.y()));
    if (m_controller != nullptr) {
        m_controller->refreshState();
    }
}

void BoardWindow::wheelEvent(QWheelEvent *event) {
    wake();
    const QPointF point = event->position();
    const bool precise = !event->pixelDelta().isNull();
    const QPointF delta =
        precise ? QPointF(event->pixelDelta())
                : QPointF(event->angleDelta()) / static_cast<qreal>(kAngleUnitsPerLine);
    if (isCameraChord(event->modifiers())) {
        m_engine.zoomScroll(static_cast<float>(point.x()), static_cast<float>(point.y()),
                            static_cast<float>(delta.y()), precise);
        return;
    }
    m_engine.panScroll(static_cast<float>(delta.x()), static_cast<float>(delta.y()), precise);
}

// The chord table below is `EditorShortcuts.swift`'s `handleEditorKeyDown`, minus text editing
// (no IME yet — phase 6) and clipboard (no OS pasteboard wiring yet — also phase 6). Bare tool
// letters go through `shortcuts::toolForKey`, the same table `qt-smoke` already pins; everything
// here is the chords that are not a tool key, ported one-for-one so a Ctrl chord on this shell
// does what the matching ⌘ chord does on the Mac.
bool BoardWindow::handleShortcut(QKeyEvent *event) {
    if (m_controller == nullptr) {
        return false;
    }
    Engine &engine = m_controller->engine();
    const bool ctrl = event->modifiers().testFlag(Qt::ControlModifier);
    const bool shift = event->modifiers().testFlag(Qt::ShiftModifier);
    const QString text = event->text().toLower();
    const char key = text.isEmpty() ? '\0' : text.front().toLatin1();

    if (ctrl && shift && key == 'z') {
        m_controller->redo();
        return true;
    }
    if (ctrl && key == 'z') {
        m_controller->undo();
        return true;
    }
    if (ctrl && shift && key == 'n') {
        m_controller->addLayer();
        return true;
    }
    if (isDeleteKey(event->key())) {
        if (ctrl) {
            if (engine.hasSelection()) {
                engine.selectionClearPixels();
            } else {
                engine.clearLayer();
            }
        } else if (engine.selectedVectorItem() >= 0) {
            engine.deleteSelectedVectorItem();
        }
        m_controller->refreshState();
        return true;
    }
    if (event->key() == Qt::Key_Escape) {
        engine.deselect();
        m_controller->refreshState();
        return true;
    }
    if (ctrl && shift && key == 'i') {
        engine.invertSelection();
        m_controller->refreshState();
        return true;
    }
    if (ctrl && key == 'a') {
        engine.selectAll();
        m_controller->refreshState();
        return true;
    }
    if (ctrl && (key == '=' || key == '+')) {
        m_controller->stepZoom(true);
        return true;
    }
    if (ctrl && key == '-') {
        m_controller->stepZoom(false);
        return true;
    }
    if (ctrl && key == 's') {
        engine.saveProject();
        return true;
    }
    if (ctrl && key == 't') {
        m_controller->toggleTransform();
        return true;
    }
    if (shift && key == 'v') {
        engine.setVectorMode(!engine.vectorMode());
        m_controller->refreshState();
        return true;
    }
    if (shift && key == 'w') {
        m_controller->setTool(CalmToolSelectColor);
        return true;
    }
    if (const auto picked = shortcuts::toolForKey(key)) {
        // The marquee key names a family; the engine's own `last_select_tool` says which member
        // was used last, the same source `AppState`'s Swift counterpart reads.
        const uint32_t tool = shortcuts::isMarqueeFamily(*picked)
                                  ? engine.state().last_select_tool
                                  : static_cast<uint32_t>(*picked);
        m_controller->setTool(tool);
        return true;
    }
    switch (key) {
    case 'f':
        m_controller->setShapeFill(!m_controller->shapeFill());
        return true;
    case 's':
        m_controller->setShapeStroke(!m_controller->shapeStroke());
        return true;
    case '0':
        m_controller->fit();
        return true;
    case '[':
    case ']': {
        const bool increase = key == ']';
        if (limits::takesEyedropperRadius(m_controller->tool())) {
            const uint32_t radius = m_controller->eyedropperRadius();
            const uint32_t clamped =
                increase ? std::min(radius + 1, limits::eyedropperRadiusMax())
                        : (radius > limits::eyedropperRadiusMin() ? radius - 1 : radius);
            m_controller->setEyedropperRadius(clamped);
        } else {
            m_controller->setBrushSize(limits::brushSizeStep(m_controller->brushSize(), increase));
        }
        return true;
    }
    default:
        return false;
    }
}

void BoardWindow::keyPressEvent(QKeyEvent *event) {
    wake();
    if (event->key() == Qt::Key_Space && !event->isAutoRepeat()) {
        m_spaceHeld = true;
        return;
    }
    if (event->key() == Qt::Key_Shift) {
        m_engine.setShift(true);
    }
    if (handleShortcut(event)) {
        return;
    }
    QWindow::keyPressEvent(event);
}

void BoardWindow::keyReleaseEvent(QKeyEvent *event) {
    wake();
    if (event->key() == Qt::Key_Space && !event->isAutoRepeat()) {
        m_spaceHeld = false;
        return;
    }
    if (event->key() == Qt::Key_Shift) {
        m_engine.setShift(false);
    }
    QWindow::keyReleaseEvent(event);
}

bool BoardWindow::event(QEvent *event) {
    switch (event->type()) {
    case QEvent::UpdateRequest:
        renderFrame();
        return true;
    case QEvent::Leave:
        m_engine.clearPointerHover();
        return true;
    case QEvent::NativeGesture: {
        // Pinch, where the platform sends it. `value()` is a relative magnification, the same
        // shape AppKit reports, so the engine takes the same factor from both shells.
        auto *gesture = static_cast<QNativeGestureEvent *>(event);
        if (gesture->gestureType() != Qt::ZoomNativeGesture) {
            break;
        }
        wake();
        const QPointF point = gesture->position();
        m_engine.zoom(static_cast<float>(point.x()), static_cast<float>(point.y()),
                      static_cast<float>(1.0 + gesture->value()));
        return true;
    }
    default:
        break;
    }
    return QWindow::event(event);
}

}  // namespace calumma
