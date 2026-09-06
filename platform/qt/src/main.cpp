#include "app/AppController.hpp"
#include "canvas/BoardWindow.hpp"
#include "engine/Engine.hpp"
#include "l10n/L10nBridge.hpp"
#include "theme/ThemeBridge.hpp"

#include <QDebug>
#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickWindow>
#include <QUrl>

// Phases 1–5 of docs/plans/01-qt-shell.md. `Engine` and `AppController` hold no Qt Quick type
// and are exercised headlessly by `./manage.py qt-smoke`; everything below this point — the
// QML engine, the singletons, the board embedding via `AppController::attachBoardHost` — is the
// part that has never run, because there is no Qt on the machine this was written on. See the
// plan for exactly which pieces that covers.
int main(int argc, char *argv[]) {
    QGuiApplication app(argc, argv);
    QGuiApplication::setApplicationName(QStringLiteral("Calumma"));
    QGuiApplication::setOrganizationName(QStringLiteral("Calumma"));

    calumma::Engine engine;
    if (!engine.isValid()) {
        qCritical() << "calumma: the engine could not open its project store";
        return 1;
    }

    calumma::ThemeBridge theme;
    calumma::L10nBridge l10n;
    l10n.setLanguage(QStringLiteral("en"));

    calumma::AppController controller(engine, theme);
    controller.setThemeMode(controller.themeMode());  // resolves the initial palette

    calumma::BoardWindow board(engine);
    board.setController(&controller);
    controller.setBoardWindow(&board);

    QQmlApplicationEngine qmlEngine;
    qmlEngine.rootContext()->setContextProperty("controller", &controller);
    qmlEngine.rootContext()->setContextProperty("theme", &theme);
    qmlEngine.rootContext()->setContextProperty("l10n", &l10n);

    QObject::connect(
        &qmlEngine, &QQmlApplicationEngine::objectCreationFailed, &app,
        [] { QGuiApplication::exit(-1); }, Qt::QueuedConnection);

    // `Main.qml`'s `BoardHost` Item calls `controller.attachBoardHost(this)` from
    // `Component.onCompleted`, which is what actually reparents `board` under the root window —
    // by the time `load` returns below, that has already happened (or failed and logged why).
    qmlEngine.load(QUrl(QStringLiteral("qrc:/qml/Main.qml")));
    if (qmlEngine.rootObjects().isEmpty()) {
        qCritical() << "calumma: Main.qml failed to load";
        return 1;
    }
    if (qobject_cast<QQuickWindow *>(qmlEngine.rootObjects().constFirst()) == nullptr) {
        qCritical() << "calumma: Main.qml's root is not a window";
        return 1;
    }

    return QGuiApplication::exec();
}
