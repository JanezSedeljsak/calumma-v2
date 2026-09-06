#include "ThemeBridge.hpp"

#include "Tokens.generated.hpp"

#include <QVariantMap>

namespace calumma {
namespace {

QColor toQColor(const tokens::Color &c) {
    return QColor::fromRgbF(c.r, c.g, c.b, c.a);
}

}  // namespace

void ThemeBridge::setDark(bool dark) {
    if (m_dark == dark) {
        return;
    }
    m_dark = dark;
    emit paletteChanged();
}

qreal ThemeBridge::radiusSm() { return tokens::Radius::sm; }
qreal ThemeBridge::radiusMd() { return tokens::Radius::md; }
qreal ThemeBridge::radiusLg() { return tokens::Radius::lg; }
qreal ThemeBridge::radiusWindow() { return tokens::Radius::window; }
qreal ThemeBridge::radiusIsland() { return tokens::Radius::island; }
qreal ThemeBridge::spaceXs() { return tokens::Space::xs; }
qreal ThemeBridge::spaceSm() { return tokens::Space::sm; }
qreal ThemeBridge::spaceMd() { return tokens::Space::md; }
qreal ThemeBridge::spaceLg() { return tokens::Space::lg; }
qreal ThemeBridge::spaceXl() { return tokens::Space::xl; }
qreal ThemeBridge::spaceXxl() { return tokens::Space::xxl; }
qreal ThemeBridge::controlHeight() { return tokens::Control::height; }
qreal ThemeBridge::labelSize() { return tokens::TypeSize::labelSize; }
qreal ThemeBridge::bodySize() { return tokens::TypeSize::bodySize; }
qreal ThemeBridge::titleSize() { return tokens::TypeSize::titleSize; }
qreal ThemeBridge::brandSize() { return tokens::TypeSize::brandSize; }

QColor ThemeBridge::bg() const { return toQColor(m_dark ? tokens::Dark::bg : tokens::Light::bg); }
QColor ThemeBridge::desk() const {
    return toQColor(m_dark ? tokens::Dark::desk : tokens::Light::desk);
}
QColor ThemeBridge::deskGrid() const {
    return toQColor(m_dark ? tokens::Dark::deskGrid : tokens::Light::deskGrid);
}
QColor ThemeBridge::paper() const {
    return toQColor(m_dark ? tokens::Dark::paper : tokens::Light::paper);
}
QColor ThemeBridge::paperBorder() const {
    return toQColor(m_dark ? tokens::Dark::paperBorder : tokens::Light::paperBorder);
}
QColor ThemeBridge::surface() const {
    return toQColor(m_dark ? tokens::Dark::surface : tokens::Light::surface);
}
QColor ThemeBridge::surfaceHover() const {
    return toQColor(m_dark ? tokens::Dark::surfaceHover : tokens::Light::surfaceHover);
}
QColor ThemeBridge::islandBorder() const {
    return toQColor(m_dark ? tokens::Dark::islandBorder : tokens::Light::islandBorder);
}
QColor ThemeBridge::controlBorder() const {
    return toQColor(m_dark ? tokens::Dark::controlBorder : tokens::Light::controlBorder);
}
QColor ThemeBridge::controlFocusBorder() const {
    return toQColor(m_dark ? tokens::Dark::controlFocusBorder : tokens::Light::controlFocusBorder);
}
QColor ThemeBridge::text() const {
    return toQColor(m_dark ? tokens::Dark::text : tokens::Light::text);
}
QColor ThemeBridge::textMuted() const {
    return toQColor(m_dark ? tokens::Dark::textMuted : tokens::Light::textMuted);
}
QColor ThemeBridge::danger() const {
    return toQColor(m_dark ? tokens::Dark::danger : tokens::Light::danger);
}
QColor ThemeBridge::accentTeal() const {
    return toQColor(m_dark ? tokens::Dark::accentTeal : tokens::Light::accentTeal);
}
QColor ThemeBridge::accentOrange() const {
    return toQColor(m_dark ? tokens::Dark::accentOrange : tokens::Light::accentOrange);
}

QVariantList ThemeBridge::presets() const {
    QVariantList out;
    for (const tokens::Preset &preset : tokens::presets) {
        QVariantMap row;
        row["id"] = QString::fromUtf8(preset.id.data(), static_cast<int>(preset.id.size()));
        row["label"] =
            QString::fromUtf8(preset.label.data(), static_cast<int>(preset.label.size()));
        row["width"] = preset.width;
        row["height"] = preset.height;
        out.append(row);
    }
    return out;
}

QColor ThemeBridge::packedToColor(uint32_t rgb) {
    return QColor(static_cast<int>((rgb >> 16) & 0xFF), static_cast<int>((rgb >> 8) & 0xFF),
                 static_cast<int>(rgb & 0xFF));
}

uint32_t ThemeBridge::colorToPacked(const QColor &color) {
    return (static_cast<uint32_t>(color.red()) << 16) |
           (static_cast<uint32_t>(color.green()) << 8) | static_cast<uint32_t>(color.blue());
}

}  // namespace calumma
