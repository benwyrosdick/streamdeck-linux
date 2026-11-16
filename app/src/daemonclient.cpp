#include "daemonclient.h"

#include <QDir>
#include <QJSEngine>
#include <QJsonDocument>
#include <QJsonObject>
#include <QMutexLocker>
#include <QProcess>
#include <QStandardPaths>

QImage FrameStore::requestImage(const QString &id, QSize *size, const QSize &requestedSize) {
    // Ids look like "<device>/key-3" with an optional "?rev" suffix.
    const QString key = id.section('?', 0, 0);
    QImage image;
    {
        QMutexLocker lock(&m_mutex);
        image = m_images.value(key);
    }
    if (image.isNull()) {
        image = QImage(1, 1, QImage::Format_ARGB32);
        image.fill(Qt::black);
    }
    if (size)
        *size = image.size();
    if (requestedSize.isValid() && !requestedSize.isEmpty())
        return image.scaled(requestedSize, Qt::KeepAspectRatio, Qt::SmoothTransformation);
    return image;
}

void FrameStore::put(const QString &key, const QImage &image) {
    QMutexLocker lock(&m_mutex);
    m_images.insert(key, image);
}

void FrameStore::clearDevice(const QString &device) {
    QMutexLocker lock(&m_mutex);
    for (auto it = m_images.begin(); it != m_images.end();) {
        if (it.key().startsWith(device + "/"))
            it = m_images.erase(it);
        else
            ++it;
    }
}

static QString defaultSocket() {
    const QString override = qEnvironmentVariable("STREAMDECK_RUNTIME_DIR");
    if (!override.isEmpty())
        return override + "/control.sock";
    QString runtime = QStandardPaths::writableLocation(QStandardPaths::RuntimeLocation);
    if (runtime.isEmpty())
        runtime = QDir::tempPath();
    return runtime + "/streamdeck-linux/control.sock";
}

DaemonClient::DaemonClient(FrameStore *frames, QObject *parent, const QString &socketPath)
    : QObject(parent), m_frames(frames), m_path(socketPath.isEmpty() ? defaultSocket() : socketPath) {
    m_reconnect.setInterval(1500);
    connect(&m_reconnect, &QTimer::timeout, this, &DaemonClient::connectNow);

    connect(&m_socket, &QLocalSocket::connected, this, [this] {
        m_reconnect.stop();
        setConnected(true);
        request("subscribe", {}, [](const QVariant &, const QString &) {});
        request("getState", {}, [this](const QVariant &r, const QString &) {
            m_state = r.toMap();
            emit stateChanged();
        });
        m_frameSocket.connectToServer(m_path);
    });
    connect(&m_socket, &QLocalSocket::disconnected, this, [this] {
        setConnected(false);
        m_frameSocket.abort();
        // Pending callbacks will never be answered; tell them so.
        const auto callbacks = m_callbacks;
        m_callbacks.clear();
        for (auto cb : callbacks)
            if (cb.isCallable())
                cb.call({QJSValue(QJSValue::NullValue), QJSValue(QStringLiteral("disconnected"))});
        const auto replies = m_replies;
        m_replies.clear();
        for (const auto &r : replies)
            r({}, QStringLiteral("disconnected"));
        m_reconnect.start();
    });
    connect(&m_socket, &QLocalSocket::errorOccurred, this, [this](QLocalSocket::LocalSocketError) {
        if (m_socket.state() != QLocalSocket::ConnectedState)
            m_reconnect.start();
    });
    connect(&m_socket, &QLocalSocket::readyRead, this, [this] {
        m_buffer += m_socket.readAll();
        qsizetype nl;
        while ((nl = m_buffer.indexOf('\n')) >= 0) {
            const QByteArray line = m_buffer.left(nl);
            m_buffer.remove(0, nl + 1);
            if (!line.trimmed().isEmpty())
                onLine(line);
        }
    });

    connect(&m_frameSocket, &QLocalSocket::connected, this, [this] {
        m_frameSocket.write(R"({"id":1,"method":"subscribeFrames","params":{}})" "\n");
    });
    connect(&m_frameSocket, &QLocalSocket::readyRead, this, [this] {
        m_frameBuffer += m_frameSocket.readAll();
        qsizetype nl;
        while ((nl = m_frameBuffer.indexOf('\n')) >= 0) {
            const QByteArray line = m_frameBuffer.left(nl);
            m_frameBuffer.remove(0, nl + 1);
            if (!line.trimmed().isEmpty())
                onFrameLine(line);
        }
    });

    connectNow();
}

DaemonClient::~DaemonClient() {
    // Closing the sockets emits disconnected(); those handlers must not run
    // against a client that is being torn down.
    m_reconnect.stop();
    m_socket.disconnect(this);
    m_frameSocket.disconnect(this);
}

void DaemonClient::connectNow() {
    if (m_socket.state() == QLocalSocket::UnconnectedState)
        m_socket.connectToServer(m_path);
}

void DaemonClient::setConnected(bool c) {
    if (c == m_connected)
        return;
    m_connected = c;
    if (!c) {
        m_state.clear();
        emit stateChanged();
    }
    emit connectedChanged();
}

void DaemonClient::call(const QString &method, const QVariantMap &params, const QJSValue &callback) {
    if (m_socket.state() != QLocalSocket::ConnectedState) {
        if (callback.isCallable())
            QJSValue(callback).call({QJSValue(QJSValue::NullValue), QJSValue(tr("The Stream Deck service is not running."))});
        return;
    }
    const qint64 id = m_next++;
    if (callback.isCallable())
        m_callbacks.insert(id, callback);
    QJsonObject req{{"id", id}, {"method", method}, {"params", QJsonObject::fromVariantMap(params)}};
    m_socket.write(QJsonDocument(req).toJson(QJsonDocument::Compact) + "\n");
}

void DaemonClient::request(const QString &method, const QVariantMap &params, Reply reply) {
    if (m_socket.state() != QLocalSocket::ConnectedState) {
        reply({}, tr("The Stream Deck service is not running."));
        return;
    }
    const qint64 id = m_next++;
    m_replies.insert(id, std::move(reply));
    QJsonObject req{{"id", id}, {"method", method}, {"params", QJsonObject::fromVariantMap(params)}};
    m_socket.write(QJsonDocument(req).toJson(QJsonDocument::Compact) + "\n");
}

void DaemonClient::onLine(const QByteArray &line) {
    const QJsonObject msg = QJsonDocument::fromJson(line).object();
    if (msg.contains("event")) {
        const QString name = msg.value("event").toString();
        const QVariant data = msg.value("data").toVariant();
        if (name == "state") {
            m_state = data.toMap();
            emit stateChanged();
        }
        emit event(name, data);
        return;
    }
    const qint64 id = msg.value("id").toInteger(-1);
    const QString error = msg.value("error").toString();
    const QVariant result = msg.value("result").toVariant();
    if (m_replies.contains(id)) {
        auto reply = m_replies.take(id);
        reply(result, error);
        return;
    }
    if (m_callbacks.contains(id)) {
        QJSValue cb = m_callbacks.take(id);
        if (m_engine)
            cb.call({m_engine->toScriptValue(result), QJSValue(error)});
    }
}

void DaemonClient::onFrameLine(const QByteArray &line) {
    const QJsonObject msg = QJsonDocument::fromJson(line).object();
    if (msg.value("event").toString() != "frame")
        return;
    const QJsonObject d = msg.value("data").toObject();
    const QString device = d.value("device").toString();
    const QString target = d.value("target").toString();
    const int index = d.value("index").toInt();
    QImage image;
    image.loadFromData(QByteArray::fromBase64(d.value("png").toString().toLatin1()), "PNG");
    const QString key = QStringLiteral("%1/%2-%3").arg(device, target).arg(index);
    m_frames->put(key, image);
    const int rev = ++m_revisions[key];
    emit frameChanged(device, target, index, rev);
}

void DaemonClient::startService() {
    // Prefer the user unit so the service keeps running after the editor quits.
    QProcess p;
    p.start("systemctl", {"--user", "start", "streamdeckd.service"});
    if (p.waitForFinished(5000) && p.exitCode() == 0)
        return;
    const QString exe = QStandardPaths::findExecutable("streamdeckd");
    if (!exe.isEmpty())
        QProcess::startDetached("setsid", {"-f", exe});
}
