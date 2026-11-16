#pragma once

#include <QHash>
#include <QJSEngine>
#include <QJSValue>
#include <QObject>
#include <QPointer>
#include <QStringList>

class QJSEngine;

// Open and save dialogs through xdg-desktop-portal, so they look like the
// rest of the desktop and work the same in a sandbox.
class PortalFilePicker : public QObject {
    Q_OBJECT

public:
    explicit PortalFilePicker(QObject *parent = nullptr) : QObject(parent) {}
    void setEngine(QJSEngine *engine) { m_engine = engine; }

    // filters: [{name: "Images", patterns: ["*.png", "*.svg"]}]
    // callback(paths): an empty list when cancelled.
    Q_INVOKABLE void open(const QString &title, const QVariantList &filters, bool directory, bool multiple, const QJSValue &callback);
    Q_INVOKABLE void save(const QString &title, const QString &suggestedName, const QVariantList &filters, const QJSValue &callback);

private slots:
    void onResponse(uint response, const QVariantMap &results);

private:
    void request(const QString &method, const QString &title, const QVariantMap &options, const QJSValue &callback);
    QHash<QString, QJSValue> m_pending;
    QPointer<QJSEngine> m_engine;
};
