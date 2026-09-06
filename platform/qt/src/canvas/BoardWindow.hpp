#pragma once

#include <QPointF>
#include <QWindow>

class QQuickWindow;
class QKeyEvent;

namespace calumma {

class Engine;
class AppController;

// The board itself: a native window wgpu presents into. Qt never paints here — there is no
// backing store and no Qt renderer on this window, only the surface the engine attached to.
//
// Phases 1, 3 and 5 of docs/plans/01-qt-shell.md: attach, present, turn platform events into
// engine calls, and — the least-verified part of this whole shell, see the plan's notes — live
// embedded as a native child under the QML scene's window rather than as its own top-level.
// No camera or document maths lives here; a position goes through as it arrived and the engine
// decides what it meant.
class BoardWindow : public QWindow {
public:
    explicit BoardWindow(Engine &engine);

    // Wired once, after construction: BoardWindow does not own an AppController's lifetime,
    // only reads it (for shortcut dispatch) and writes to it (`refreshState` after a frame, so
    // the chrome's zoom pill and undo/redo buttons stay live during a drag without QML polling).
    void setController(AppController *controller) { m_controller = controller; }

    // Reparents this window as a native child of `host` and matches `AppController`'s
    // `attachBoardHost` — see the QML `Item` that calls it. `host` is the QQuickWindow the QML
    // scene lives in, found via a placeholder Item's `window()`; this window keeps its own
    // identity (still a `QWindow`, still what `attachOnce` calls `winId()` on) but presents
    // inside the chrome's window instead of floating beside it.
    void embedUnder(QQuickWindow *host);

protected:
    void exposeEvent(QExposeEvent *event) override;
    void resizeEvent(QResizeEvent *event) override;
    void mousePressEvent(QMouseEvent *event) override;
    void mouseMoveEvent(QMouseEvent *event) override;
    void mouseReleaseEvent(QMouseEvent *event) override;
    void wheelEvent(QWheelEvent *event) override;
    void keyPressEvent(QKeyEvent *event) override;
    void keyReleaseEvent(QKeyEvent *event) override;
    bool event(QEvent *event) override;

private:
    // Attaches on the first expose, when the native window behind this QWindow exists. Doing it
    // in the constructor would ask the platform for a handle it has not made yet — the Qt
    // counterpart of the macOS shell attaching in viewDidMoveToWindow.
    bool attachOnce();
    void renderFrame();
    void scheduleFrame();
    // An event arrived, so draw at the display's rate until the engine says it may slow down.
    // This only ever speeds the board up: the first frame after a rest is the one the pointer
    // is waiting on, and it should not have to wait out an idle interval first.
    void wake();

    // Ctrl (⌘ on the Mac) or Alt with a drag or a wheel means camera, not ink — the one chord
    // the shell reads for itself, because it decides which engine call an event becomes.
    static bool isCameraChord(Qt::KeyboardModifiers modifiers);

    // The non-tool-key chords (undo, save, fit, …). Returns whether the event was consumed, the
    // same contract `handleEditorKeyDown` has on the Swift side. A no-op without a controller.
    bool handleShortcut(QKeyEvent *event);

    Engine &m_engine;
    AppController *m_controller = nullptr;
    bool m_attached = false;
    bool m_attachFailed = false;
    bool m_framePending = false;
    bool m_awake = true;
    int m_displayCeiling = 60;

    bool m_strokeActive = false;
    bool m_panning = false;
    bool m_spaceHeld = false;
    QPointF m_lastPanPoint;
};

}  // namespace calumma
