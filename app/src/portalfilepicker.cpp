#include "portalfilepicker.h"

#include <QDBusArgument>
#include <QDBusConnection>
#include <QDBusMessage>
#include <QDBusMetaType>
#include <QDBusObjectPath>
#include <QDBusPendingCallWatcher>
#include <QDBusPendingReply>
#include <QJSEngine>
#include <QRandomGenerator>
#include <QUrl>

// The portal's filter type is a(sa(us)): a name and a list of (kind, pattern)
// where kind 0 is a glob and 1 a MIME type.
struct PortalFilterRule {
    uint kind = 0;
    QString pattern;
};
struct PortalFilter {
    QString name;
    QList<PortalFilterRule> rules;
};
Q_DECLARE_METATYPE(PortalFilterRule)
Q_DECLARE_METATYPE(PortalFilter)
Q_DECLARE_METATYPE(QList<PortalFilter>)

QDBusArgument &operator<<(QDBusArgument &arg, const PortalFilterRule &r) {
    arg.beginStructure();
    arg << r.kind << r.pattern;
    arg.endStructure();
    return arg;
}
const QDBusArgument &operator>>(const QDBusArgument &arg, PortalFilterRule &r) {
    arg.beginStructure();
    arg >> r.kind >> r.pattern;
    arg.endStructure();
    return arg;
}
QDBusArgument &operator<<(QDBusArgument &arg, const PortalFilter &f) {
    arg.beginStructure();
    arg << f.name << f.rules;
    arg.endStructure();
    return arg;
}
const QDBusArgument &operator>>(const QDBusArgument &arg, PortalFilter &f) {
    arg.beginStructure();
    arg >> f.name >> f.rules;
    arg.endStructure();
    return arg;
}

static void registerTypes() {
    static bool done = false;
    if (done)
        return;
    done = true;
    qDBusRegisterMetaType<PortalFilterRule>();
    qDBusRegisterMetaType<QList<PortalFilterRule>>();
    qDBusRegisterMetaType<PortalFilter>();
    qDBusRegisterMetaType<QList<PortalFilter>>();
}

static QList<PortalFilter> toFilters(const QVariantList &list) {
    QList<PortalFilter> out;
    for (const auto &v : list) {
        const auto m = v.toMap();
        PortalFilter f;
        f.name = m.value("name").toString();
        for (const auto &p : m.value("patterns").toList())
            f.rules.append({0, p.toString()});
        if (!f.rules.isEmpty())
            out.append(f);
    }
    return out;
}

void PortalFilePicker::open(const QString &title, const QVariantList &filters, bool directory, bool multiple, const QJSValue &callback) {
    registerTypes();
    QVariantMap options;
    options["multiple"] = multiple;
    options["directory"] = directory;
    const auto f = toFilters(filters);
    if (!f.isEmpty())
        options["filters"] = QVariant::fromValue(f);
    request("OpenFile", title, options, callback);
}

void PortalFilePicker::save(const QString &title, const QString &suggestedName, const QVariantList &filters, const QJSValue &callback) {
    registerTypes();
    QVariantMap options;
    options["current_name"] = suggestedName;
    const auto f = toFilters(filters);
    if (!f.isEmpty())
        options["filters"] = QVariant::fromValue(f);
    request("SaveFile", title, options, callback);
}

void PortalFilePicker::request(const QString &method, const QString &title, const QVariantMap &opts, const QJSValue &callback) {
    auto bus = QDBusConnection::sessionBus();
    QVariantMap options = opts;
    const QString token = QStringLiteral("streamdeck%1").arg(QRandomGenerator::global()->generate());
    options["handle_token"] = token;
    // Subscribe to the Response before calling, using the handle path the
    // portal will use, so a fast reply cannot be missed.
    QString sender = bus.baseService().mid(1).replace('.', '_');
    const QString handle = QStringLiteral("/org/freedesktop/portal/desktop/request/%1/%2").arg(sender, token);
    m_pending.insert(handle, callback);
    bus.connect("org.freedesktop.portal.Desktop", handle, "org.freedesktop.portal.Request", "Response", this, SLOT(onResponse(uint, QVariantMap)));

    QDBusMessage msg = QDBusMessage::createMethodCall("org.freedesktop.portal.Desktop", "/org/freedesktop/portal/desktop", "org.freedesktop.portal.FileChooser", method);
    msg << QString() << title << options;
    auto *watcher = new QDBusPendingCallWatcher(bus.asyncCall(msg), this);
    connect(watcher, &QDBusPendingCallWatcher::finished, this, [this, handle](QDBusPendingCallWatcher *w) {
        QDBusPendingReply<QDBusObjectPath> reply = *w;
        w->deleteLater();
        if (reply.isError()) {
            QJSValue cb = m_pending.take(handle);
            if (cb.isCallable())
                cb.call({QJSValue(QJSValue::NullValue)});
            qWarning("file portal: %s", qPrintable(reply.error().message()));
        }
    });
}

void PortalFilePicker::onResponse(uint response, const QVariantMap &results) {
    const QDBusMessage msg = QDBusMessage();
    Q_UNUSED(msg);
    // The handle is the signal's object path; Qt does not pass it to the slot,
    // but only one dialog is open at a time, so the oldest pending wins.
    if (m_pending.isEmpty())
        return;
    const QString handle = m_pending.keys().first();
    QJSValue cb = m_pending.take(handle);
    QDBusConnection::sessionBus().disconnect("org.freedesktop.portal.Desktop", handle, "org.freedesktop.portal.Request", "Response", this, SLOT(onResponse(uint, QVariantMap)));
    QStringList paths;
    if (response == 0) {
        for (const auto &u : results.value("uris").toStringList())
            paths << QUrl(u).toLocalFile();
    }
    if (cb.isCallable() && m_engine)
        cb.call({m_engine->toScriptValue(paths)});
}
