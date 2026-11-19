#include "fonts.h"

#include <QtGui/QFont>
#include <QtGui/QFontDatabase>
#include <QtGui/QGuiApplication>

// The Blockworked typeface (Atkinson Hyperlegible Next, OFL, res/fonts), so the
// editor reads the same on every desktop. Falls back to the platform font.
void stitchcraft_use_bundled_font()
{
    static const char *const files[] = {"Regular", "Medium", "SemiBold", "Bold", "Italic"};
    QString family;
    for (const char *name : files) {
        const int id = QFontDatabase::addApplicationFont(
            QStringLiteral(":/fonts/AtkinsonHyperlegibleNext-%1.ttf").arg(QLatin1String(name)));
        if (id >= 0 && family.isEmpty())
            family = QFontDatabase::applicationFontFamilies(id).value(0);
    }
    if (family.isEmpty()) {
        qWarning("Stitchcraft: bundled font missing from :/fonts, using the platform font");
        return;
    }
    QFont font = QGuiApplication::font();
    font.setFamilies({family});
    QGuiApplication::setFont(font);
}
