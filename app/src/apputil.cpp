#include "apputil.h"

#include <QDesktopServices>
#include <QDir>
#include <QFileInfo>
#include <QJsonDocument>
#include <QKeySequence>
#include <QProcess>
#include <QStandardPaths>
#include <QUuid>

bool AppUtil::exists(const QString &path) const {
    return QFileInfo::exists(path);
}

QString AppUtil::homePath() const {
    return QDir::homePath();
}

QString AppUtil::dataPath() const {
    const QString o = qEnvironmentVariable("STREAMDECK_DATA_DIR");
    if (!o.isEmpty())
        return o;
    return QStandardPaths::writableLocation(QStandardPaths::GenericDataLocation) + "/streamdeck-linux";
}

void AppUtil::openUrl(const QString &url) const {
    QDesktopServices::openUrl(QUrl::fromUserInput(url));
}

bool AppUtil::launch(const QString &program, const QStringList &args) const {
    return QProcess::startDetached(program, args);
}

QString AppUtil::keyName(int qtKey, int modifiers) const {
    QStringList parts;
    // Spelled the way Omarchy's own keybinding hints spell them.
    if (modifiers & Qt::MetaModifier)
        parts << "Super";
    if (modifiers & Qt::ControlModifier)
        parts << "Ctrl";
    if (modifiers & Qt::AltModifier)
        parts << "Alt";
    if (modifiers & Qt::ShiftModifier)
        parts << "Shift";
    const bool modifierKey = qtKey == Qt::Key_Shift || qtKey == Qt::Key_Control || qtKey == Qt::Key_Meta || qtKey == Qt::Key_Alt || qtKey == Qt::Key_Super_L || qtKey == Qt::Key_Super_R;
    if (qtKey && qtKey != Qt::Key_unknown && !modifierKey)
        parts << QKeySequence(qtKey).toString(QKeySequence::NativeText);
    return parts.join(" + ");
}

QString AppUtil::uuid() const {
    return QUuid::createUuid().toString(QUuid::WithoutBraces);
}

QVariant AppUtil::parseJson(const QString &text) const {
    return QJsonDocument::fromJson(text.toUtf8()).toVariant();
}

QString AppUtil::toJson(const QVariant &value) const {
    return QString::fromUtf8(QJsonDocument::fromVariant(value).toJson(QJsonDocument::Compact));
}
