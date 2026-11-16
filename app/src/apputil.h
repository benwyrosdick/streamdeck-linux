#pragma once

#include <QObject>
#include <QUrl>
#include <QVariant>

// Small helpers QML cannot do on its own.
class AppUtil : public QObject {
    Q_OBJECT

public:
    explicit AppUtil(QObject *parent = nullptr) : QObject(parent) {}

    Q_INVOKABLE QUrl fileUrl(const QString &path) const { return QUrl::fromLocalFile(path); }
    Q_INVOKABLE QString localPath(const QUrl &url) const { return url.isLocalFile() ? url.toLocalFile() : url.toString(); }
    Q_INVOKABLE bool exists(const QString &path) const;
    Q_INVOKABLE QString homePath() const;
    Q_INVOKABLE QString dataPath() const;
    Q_INVOKABLE void openUrl(const QString &url) const;
    Q_INVOKABLE bool launch(const QString &program, const QStringList &args = {}) const;
    // The original app stores hotkeys by Qt key code; QML hands us a key event's parts.
    Q_INVOKABLE QString keyName(int qtKey, int modifiers) const;
    Q_INVOKABLE QString uuid() const;
    Q_INVOKABLE QString env(const QString &name) const { return qEnvironmentVariable(name.toUtf8().constData()); }
    Q_INVOKABLE QVariant parseJson(const QString &text) const;
    Q_INVOKABLE QString toJson(const QVariant &value) const;
};
