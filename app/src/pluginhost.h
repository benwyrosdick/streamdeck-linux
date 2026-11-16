#pragma once

#include <QObject>
#include <QStringList>

class QWebEnginePage;

// Runs a plugin whose CodePath is an HTML page. streamdeckd starts
// `streamdeck plugin-host <page> -port … -pluginUUID … -registerEvent … -info …`;
// this loads the page in an invisible web engine page and calls its
// connectElgatoStreamDeckSocket(), exactly as the original app does.
class PluginHost : public QObject {
    Q_OBJECT

public:
    explicit PluginHost(const QStringList &args, QObject *parent = nullptr);
    bool valid() const { return !m_page.isEmpty() && m_port > 0; }

private:
    QString m_page;
    int m_port = 0;
    QString m_uuid;
    QString m_registerEvent;
    QString m_info;
    QWebEnginePage *m_webPage = nullptr;
};
