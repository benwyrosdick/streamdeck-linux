#include "pluginhost.h"

#include <QCoreApplication>
#include <QJsonArray>
#include <QJsonDocument>
#include <QUrl>
#include <QWebEnginePage>
#include <QWebEngineProfile>
#include <QWebEngineSettings>

static QString jsString(const QString &s) {
    // JSON string encoding is a valid JavaScript string literal.
    return QString::fromUtf8(QJsonDocument(QJsonArray{s}).toJson(QJsonDocument::Compact)).mid(1).chopped(1);
}

PluginHost::PluginHost(const QStringList &args, QObject *parent) : QObject(parent) {
    for (int i = 0; i < args.size(); ++i) {
        const QString a = args.at(i);
        const QString next = i + 1 < args.size() ? args.at(i + 1) : QString();
        if (a == "-port") {
            m_port = next.toInt();
            ++i;
        } else if (a == "-pluginUUID") {
            m_uuid = next;
            ++i;
        } else if (a == "-registerEvent") {
            m_registerEvent = next;
            ++i;
        } else if (a == "-info") {
            m_info = next;
            ++i;
        } else if (m_page.isEmpty() && !a.startsWith('-')) {
            m_page = a;
        }
    }
    if (!valid())
        return;

    // Each HTML plugin keeps its own storage, so localStorage survives restarts.
    auto *profile = new QWebEngineProfile(QStringLiteral("plugin-") + m_uuid, this);
    m_webPage = new QWebEnginePage(profile, this);
    auto *s = m_webPage->settings();
    s->setAttribute(QWebEngineSettings::LocalContentCanAccessRemoteUrls, true);
    s->setAttribute(QWebEngineSettings::LocalContentCanAccessFileUrls, true);
    s->setAttribute(QWebEngineSettings::LocalStorageEnabled, true);
    s->setAttribute(QWebEngineSettings::PlaybackRequiresUserGesture, false);

    connect(m_webPage, &QWebEnginePage::loadFinished, this, [this](bool ok) {
        if (!ok) {
            qWarning("plugin page failed to load: %s", qPrintable(m_page));
            QCoreApplication::exit(1);
            return;
        }
        const QString js = QStringLiteral("connectElgatoStreamDeckSocket(%1, %2, %3, %4);")
                               .arg(m_port)
                               .arg(jsString(m_uuid), jsString(m_registerEvent), jsString(m_info));
        m_webPage->runJavaScript(js);
    });
    m_webPage->load(QUrl::fromLocalFile(m_page));
}
