#pragma once

#include <Calumma.hpp>

#include <QString>

class QWindow;

namespace calumma {

struct NativeSurfaceResult {
    CalmNativeSurface surface{};
    bool ok = false;
    // Why the platform could not be described, for the one log line the caller prints.
    QString reason;
};

// Reads the handles a realized QWindow sits on and states them in the engine's terms. The
// window must already have a native window behind it (QWindow::create(), or the first
// exposeEvent) — winId() on an unrealized window creates one as a side effect, which is not
// something to rely on while a surface is being attached.
NativeSurfaceResult nativeSurfaceFor(QWindow *window);

}  // namespace calumma
