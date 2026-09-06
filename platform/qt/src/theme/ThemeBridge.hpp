#pragma once

#include <QColor>
#include <QObject>
#include <QVariantList>

namespace calumma {

// The QML-facing half of `Tokens.generated.hpp`: the same numbers, resolved against whichever
// palette (`tokens::Light` or `tokens::Dark`) is active, and reachable from QML as a singleton
// (`Theme.desk`, `Theme.spaceMd`, …) rather than a `#include` no `.qml` file can do.
//
// Values that never change with the theme (radius, space, control height, type sizes) are
// `Q_PROPERTY CONSTANT`; colours are re-emitted through `paletteChanged` when `dark` flips, and
// nothing here decides *whether* the app is dark — `AppController::setTheme` (mirroring
// `AppState::resolvedDark`) does that and calls `setDark`.
class ThemeBridge : public QObject {
    Q_OBJECT
    Q_PROPERTY(bool dark READ dark WRITE setDark NOTIFY paletteChanged)

    Q_PROPERTY(qreal radiusSm READ radiusSm CONSTANT)
    Q_PROPERTY(qreal radiusMd READ radiusMd CONSTANT)
    Q_PROPERTY(qreal radiusLg READ radiusLg CONSTANT)
    Q_PROPERTY(qreal radiusWindow READ radiusWindow CONSTANT)
    Q_PROPERTY(qreal radiusIsland READ radiusIsland CONSTANT)

    Q_PROPERTY(qreal spaceXs READ spaceXs CONSTANT)
    Q_PROPERTY(qreal spaceSm READ spaceSm CONSTANT)
    Q_PROPERTY(qreal spaceMd READ spaceMd CONSTANT)
    Q_PROPERTY(qreal spaceLg READ spaceLg CONSTANT)
    Q_PROPERTY(qreal spaceXl READ spaceXl CONSTANT)
    Q_PROPERTY(qreal spaceXxl READ spaceXxl CONSTANT)

    Q_PROPERTY(qreal controlHeight READ controlHeight CONSTANT)

    Q_PROPERTY(qreal labelSize READ labelSize CONSTANT)
    Q_PROPERTY(qreal bodySize READ bodySize CONSTANT)
    Q_PROPERTY(qreal titleSize READ titleSize CONSTANT)
    Q_PROPERTY(qreal brandSize READ brandSize CONSTANT)

    Q_PROPERTY(QColor bg READ bg NOTIFY paletteChanged)
    Q_PROPERTY(QColor desk READ desk NOTIFY paletteChanged)
    Q_PROPERTY(QColor deskGrid READ deskGrid NOTIFY paletteChanged)
    Q_PROPERTY(QColor paper READ paper NOTIFY paletteChanged)
    Q_PROPERTY(QColor paperBorder READ paperBorder NOTIFY paletteChanged)
    Q_PROPERTY(QColor surface READ surface NOTIFY paletteChanged)
    Q_PROPERTY(QColor surfaceHover READ surfaceHover NOTIFY paletteChanged)
    Q_PROPERTY(QColor islandBorder READ islandBorder NOTIFY paletteChanged)
    Q_PROPERTY(QColor controlBorder READ controlBorder NOTIFY paletteChanged)
    Q_PROPERTY(QColor controlFocusBorder READ controlFocusBorder NOTIFY paletteChanged)
    Q_PROPERTY(QColor text READ text NOTIFY paletteChanged)
    Q_PROPERTY(QColor textMuted READ textMuted NOTIFY paletteChanged)
    Q_PROPERTY(QColor danger READ danger NOTIFY paletteChanged)
    Q_PROPERTY(QColor accentTeal READ accentTeal NOTIFY paletteChanged)
    Q_PROPERTY(QColor accentOrange READ accentOrange NOTIFY paletteChanged)

public:
    explicit ThemeBridge(QObject *parent = nullptr) : QObject(parent) {}

    bool dark() const noexcept { return m_dark; }
    void setDark(bool dark);

    static qreal radiusSm();
    static qreal radiusMd();
    static qreal radiusLg();
    static qreal radiusWindow();
    static qreal radiusIsland();
    static qreal spaceXs();
    static qreal spaceSm();
    static qreal spaceMd();
    static qreal spaceLg();
    static qreal spaceXl();
    static qreal spaceXxl();
    static qreal controlHeight();
    static qreal labelSize();
    static qreal bodySize();
    static qreal titleSize();
    static qreal brandSize();

    QColor bg() const;
    QColor desk() const;
    QColor deskGrid() const;
    QColor paper() const;
    QColor paperBorder() const;
    QColor surface() const;
    QColor surfaceHover() const;
    QColor islandBorder() const;
    QColor controlBorder() const;
    QColor controlFocusBorder() const;
    QColor text() const;
    QColor textMuted() const;
    QColor danger() const;
    QColor accentTeal() const;
    QColor accentOrange() const;

    // The new-project size presets, as a QML-friendly list of {id, label, width, height}.
    Q_INVOKABLE QVariantList presets() const;

    // 0xRRGGBB, the same shape `calm_palette_color` and `CalmProjectInfo.accent` use — the one
    // conversion QML needs and C++ does not otherwise, since the wrapper deals in `uint32_t`.
    Q_INVOKABLE static QColor packedToColor(uint32_t rgb);
    Q_INVOKABLE static uint32_t colorToPacked(const QColor &color);

signals:
    void paletteChanged();

private:
    bool m_dark = false;
};

}  // namespace calumma
