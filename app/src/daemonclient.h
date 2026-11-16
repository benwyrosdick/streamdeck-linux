#pragma once

#include <QHash>
#include <QImage>
#include <QJSEngine>
#include <QJSValue>
#include <QLocalSocket>
#include <QMutex>
#include <QObject>
#include <QPointer>
#include <QQuickImageProvider>
#include <QTimer>
#include <QVariant>



// Latest rendered image of every key and screen, served to QML as
// image://frames/<device>/<target>-<index>. QML appends a revision to the
// URL to make Image reload when a frame changes.
class FrameStore : public QQuickImageProvider {
public:
    FrameStore() : QQuickImageProvider(QQuickImageProvider::Image) {}
    QImage requestImage(const QString &id, QSize *size, const QSize &requestedSize) override;
    void put(const QString &key, const QImage &image);
    void clearDevice(const QString &device);

private:
    QMutex m_mutex;
    QHash<QString, QImage> m_images;
};

// Talks to streamdeckd over its control socket. Requests carry an id and a
// JavaScript callback; events and frames arrive on two separate connections
// so a burst of key images never delays a reply.
class DaemonClient : public QObject {
    Q_OBJECT
    Q_PROPERTY(bool connected READ connected NOTIFY connectedChanged)
    Q_PROPERTY(QVariantMap state READ state NOTIFY stateChanged)
    Q_PROPERTY(QString socketPath READ socketPath CONSTANT)

public:
    explicit DaemonClient(FrameStore *frames, QObject *parent = nullptr, const QString &socketPath = {});
    ~DaemonClient() override;

    void setEngine(QJSEngine *engine) { m_engine = engine; }
    bool connected() const { return m_connected; }
    QVariantMap state() const { return m_state; }
    QString socketPath() const { return m_path; }

    // callback(result, error): error is an empty string on success.
    Q_INVOKABLE void call(const QString &method, const QVariantMap &params = {}, const QJSValue &callback = QJSValue());
    // Starts the service through systemd, or directly when no unit is installed.
    Q_INVOKABLE void startService();
    Q_INVOKABLE int frameRevision(const QString &key) const { return m_revisions.value(key); }

    // Same-process helpers for C++ callers (tests, plugin host).
    using Reply = std::function<void(const QVariant &result, const QString &error)>;
    void request(const QString &method, const QVariantMap &params, Reply reply);

signals:
    void connectedChanged();
    void stateChanged();
    void event(const QString &name, const QVariant &data);
    void frameChanged(const QString &device, const QString &target, int index, int revision);

private:
    void connectNow();
    void onLine(const QByteArray &line);
    void onFrameLine(const QByteArray &line);
    void setConnected(bool c);

    FrameStore *m_frames;
    QString m_path;
    QLocalSocket m_socket;
    QLocalSocket m_frameSocket;
    QByteArray m_buffer;
    QByteArray m_frameBuffer;
    QTimer m_reconnect;
    bool m_connected = false;
    qint64 m_next = 1;
    QHash<qint64, QJSValue> m_callbacks;
    QHash<qint64, Reply> m_replies;
    QHash<QString, int> m_revisions;
    QVariantMap m_state;
    // Set when a state event arrives, so a slower initial getState reply
    // cannot overwrite newer state.
    bool m_stateFromEvent = false;
    QPointer<QJSEngine> m_engine;
};
