#include "opener.h"

#include <QCoreApplication>
#include <QDBusConnection>
#include <QDBusMessage>
#include <QEventLoop>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QLocalSocket>
#include <QNetworkAccessManager>
#include <QNetworkReply>
#include <QTimer>
#include <QUuid>
#include <cstdio>

namespace Opener {

// Elgato's public marketplace API; looking an item up needs no account.
static const char *kMarketplace = "https://mp-gateway.elgato.com";

static QString sentence(QString s) {
    if (!s.isEmpty())
        s[0] = s[0].toUpper();
    if (!s.isEmpty() && !s.endsWith('.'))
        s += '.';
    return s;
}

QString marketplaceItem(const QString &url) {
    static const QString prefix = "streamdeck://plugins/install/";
    if (!url.startsWith(prefix))
        return {};
    const QString id = url.mid(prefix.size()).section('/', 0, 0).section('?', 0, 0).section('#', 0, 0);
    return QUuid::fromString(id).isNull() ? QString() : id.toLower();
}

Outcome pluginInstalled(const QVariantMap &plugin) {
    const QString name = plugin.value("name").toString().isEmpty() ? plugin.value("uuid").toString() : plugin.value("name").toString();
    const QString why = plugin.value("unsupported").toString();
    if (!why.isEmpty())
        return {false, QString("%1 can't run on Linux").arg(name), QString("It was installed, but %1%2").arg(why, why.endsWith('.') ? "" : ".")};
    const QString version = plugin.value("version").toString();
    return {true, QString("Installed %1").arg(name),
            (version.isEmpty() ? QString() : QString("Version %1. ").arg(version)) + "Its actions are in the editor's action list."};
}

Outcome marketplaceLink(const QVariantMap &item, const QVariantMap &installed) {
    const QString name = item.value("product_name").toString();
    if (!installed.isEmpty()) {
        const QString version = installed.value("version").toString();
        return {true, QString("%1 is already installed").arg(name), version.isEmpty() ? QString() : QString("Version %1.").arg(version)};
    }
    return {false, QString("Download %1 from the marketplace").arg(name),
            "Elgato requires signing in to download, so this app can't fetch it. "
            "Download the .streamDeckPlugin file from its marketplace page, then open it."};
}

static void notify(const Outcome &o) {
    fprintf(o.ok ? stdout : stderr, "%s%s%s\n", qPrintable(o.summary), o.body.isEmpty() ? "" : ": ", qPrintable(o.body));
    QDBusMessage m = QDBusMessage::createMethodCall("org.freedesktop.Notifications", "/org/freedesktop/Notifications",
                                                    "org.freedesktop.Notifications", "Notify");
    // Urgency 2 (critical) keeps failures on screen until dismissed.
    const QVariantMap hints{{"urgency", QVariant::fromValue<uchar>(o.ok ? 1 : 2)}, {"desktop-entry", "streamdeck"}};
    m << QString("Stream Deck") << 0u << QString("streamdeck") << o.summary << o.body << QStringList() << hints << (o.ok ? 6000 : -1);
    QDBusConnection::sessionBus().call(m, QDBus::Block, 2000);
}

// One request to streamdeckd over its control socket.
class Control {
public:
    explicit Control(const QString &path) { m_socket.connectToServer(path); }
    bool connected() { return m_socket.waitForConnected(2000); }

    QVariant call(const QString &method, const QVariantMap &params, QString *error) {
        const QVariantMap req{{"id", ++m_id}, {"method", method}, {"params", params}};
        m_socket.write(QJsonDocument::fromVariant(req).toJson(QJsonDocument::Compact) + "\n");
        m_socket.flush();
        while (!m_socket.canReadLine()) {
            if (!m_socket.waitForReadyRead(15000)) {
                *error = "no reply from streamdeckd";
                return {};
            }
        }
        const QVariantMap reply = QJsonDocument::fromJson(m_socket.readLine()).toVariant().toMap();
        *error = reply.value("error").toString();
        return reply.value("result");
    }

    QVariantMap plugin(const QString &uuid) {
        QString error;
        for (const QVariant &p : call("listPlugins", {}, &error).toList())
            if (p.toMap().value("uuid").toString() == uuid)
                return p.toMap();
        return {};
    }

private:
    QLocalSocket m_socket;
    int m_id = 0;
};

static QVariantMap lookUpItem(const QString &id, QString *error) {
    QNetworkAccessManager net;
    QNetworkRequest req(QUrl(QString("%1/variants?ids=%2").arg(kMarketplace, id)));
    req.setTransferTimeout(10000);
    QNetworkReply *reply = net.get(req);
    QEventLoop loop;
    QObject::connect(reply, &QNetworkReply::finished, &loop, &QEventLoop::quit);
    loop.exec();
    reply->deleteLater();
    if (reply->error() != QNetworkReply::NoError) {
        *error = reply->errorString();
        return {};
    }
    const QVariantList results = QJsonDocument::fromJson(reply->readAll()).toVariant().toMap().value("results").toList();
    if (results.isEmpty()) {
        *error = "the marketplace does not know this item";
        return {};
    }
    return results.first().toMap();
}

int run(const QString &target, const QString &socketPath) {
    Control daemon(socketPath);
    if (!daemon.connected()) {
        notify({false, "The Stream Deck service isn't running", "Start it with: systemctl --user start streamdeckd"});
        return 1;
    }
    QString error;
    Outcome outcome;

    if (const QString item = marketplaceItem(target); !item.isEmpty()) {
        const QVariantMap found = lookUpItem(item, &error);
        if (!error.isEmpty()) {
            notify({false, "Couldn't look up the marketplace item", sentence(error)});
            return 1;
        }
        outcome = marketplaceLink(found, daemon.plugin(found.value("streamdeckid").toString()));
        notify(outcome);
        return 0;
    }

    if (target.startsWith("streamdeck://")) {
        daemon.call("deepLink", {{"url", target}}, &error);
        if (!error.isEmpty()) {
            notify({false, "Couldn't open the link", sentence(error)});
            return 1;
        }
        return 0;
    }

    const QString path = QFileInfo(target).absoluteFilePath();
    const QString file = QFileInfo(path).fileName();
    if (path.endsWith(".streamDeckPlugin") || path.endsWith(".sdPlugin")) {
        const QString uuid = daemon.call("installPlugin", {{"path", path}}, &error).toString();
        if (!error.isEmpty()) {
            notify({false, QString("Couldn't install %1").arg(file), sentence(error)});
            return 1;
        }
        outcome = pluginInstalled(daemon.plugin(uuid).isEmpty() ? QVariantMap{{"uuid", uuid}} : daemon.plugin(uuid));
    } else if (path.endsWith(".streamDeckIconPack") || path.endsWith(".sdIconPack")) {
        const QString id = daemon.call("installIconPack", {{"path", path}}, &error).toString();
        if (!error.isEmpty()) {
            notify({false, QString("Couldn't install %1").arg(file), sentence(error)});
            return 1;
        }
        QString name = id;
        for (const QVariant &p : daemon.call("listIconPacks", {}, &error).toList())
            if (p.toMap().value("id").toString() == id)
                name = p.toMap().value("name").toString();
        outcome = {true, QString("Installed the %1 icon pack").arg(name), "Its icons are in the key image picker."};
    } else {
        const QStringList ids = daemon.call("importProfile", {{"path", path}}, &error).toStringList();
        if (!error.isEmpty()) {
            notify({false, QString("Couldn't import %1").arg(file), sentence(error)});
            return 1;
        }
        QStringList names;
        for (const QVariant &p : daemon.call("listProfiles", {}, &error).toList())
            if (ids.contains(p.toMap().value("id").toString()))
                names << p.toMap().value("name").toString();
        outcome = {true, names.size() == 1 ? QString("Imported %1").arg(names.first()) : QString("Imported %1 profiles").arg(ids.size()),
                   names.size() == 1 ? "Pick it from the profile list in the editor." : "Pick them from the profile list in the editor."};
    }
    notify(outcome);
    return outcome.ok ? 0 : 1;
}

} // namespace Opener
