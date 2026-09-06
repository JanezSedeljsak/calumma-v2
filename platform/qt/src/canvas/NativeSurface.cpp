#include "NativeSurface.hpp"

#include <QGuiApplication>
#include <QWindow>

#if defined(Q_OS_LINUX)
#include <QtGui/qguiapplication_platform.h>
#endif

namespace calumma {
namespace {

NativeSurfaceResult failed(const QString &reason) {
    NativeSurfaceResult result;
    result.reason = reason;
    return result;
}

NativeSurfaceResult described(CalmSurfaceKind kind, void *display, void *window) {
    NativeSurfaceResult result;
    result.surface.kind = kind;
    result.surface.display = display;
    result.surface.window = window;
    result.ok = true;
    return result;
}

}  // namespace

NativeSurfaceResult nativeSurfaceFor(QWindow *window) {
    if (window == nullptr || !window->handle()) {
        return failed(QStringLiteral("the window has no native handle yet"));
    }
    const WId id = window->winId();
    if (id == 0) {
        return failed(QStringLiteral("the platform gave the window no id"));
    }

#if defined(Q_OS_WIN)
    return described(CalmSurfaceKindWin32Hwnd, nullptr, reinterpret_cast<void *>(id));
#elif defined(Q_OS_LINUX)
    // An XID travels in the pointer's bits rather than as an address — see the note on
    // CalmNativeSurface::window. Wayland is deliberately not handled here yet: the wl_surface
    // of a QWindow is not reachable through public Qt API, which is the open question plan 01
    // records against this file. Until it is settled, run under xcb.
    if (auto *x11 = qGuiApp->nativeInterface<QNativeInterface::QX11Application>()) {
        return described(CalmSurfaceKindXlib, x11->display(), reinterpret_cast<void *>(id));
    }
    return failed(QStringLiteral("only the xcb platform plugin is wired up so far — run with "
                                 "QT_QPA_PLATFORM=xcb"));
#else
    return failed(QStringLiteral("no native surface mapping for this platform"));
#endif
}

}  // namespace calumma
