#include <QBuffer>
#include <QDir>
#include <QFile>
#include <QImage>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLocalServer>
#include <QLocalSocket>
#include <QQmlEngine>
#include <QTemporaryDir>
#include <QtTest>

#include "apputil.h"
#include "daemonclient.h"
#include "theme.h"
#include "translator.h"

// A stand-in for streamdeckd: answers requests and can push events.
class FakeDaemon : public QObject {
    Q_OBJECT
public:
    explicit FakeDaemon(const QString &path) {
        QLocalServer::removeServer(path);
        server.listen(path);
        connect(&server, &QLocalServer::newConnection, this, [this] {
            while (auto *s = server.nextPendingConnection()) {
                clients << s;
                connect(s, &QLocalSocket::readyRead, this, [this, s] {
                    while (s->canReadLine()) {
                        const auto req = QJsonDocument::fromJson(s->readLine()).object();
                        const QString method = req.value("method").toString();
                        methods << method;
                        QJsonObject reply{{"id", req.value("id")}};
                        if (method == "getState")
                            reply["result"] = QJsonObject{{"devices", QJsonArray{QJsonObject{{"id", "virtual-1"}, {"name", "Deck"}}}}};
                        else if (method == "fail")
                            reply["error"] = "nope";
                        else if (method == "subscribeFrames")
                            frameSocket = s, reply["result"] = true;
                        else
                            reply["result"] = QJsonObject{{"echo", req.value("params")}};
                        s->write(QJsonDocument(reply).toJson(QJsonDocument::Compact) + "\n");
                    }
                });
            }
        });
    }
    void push(QLocalSocket *s, const QJsonObject &event) {
        s->write(QJsonDocument(event).toJson(QJsonDocument::Compact) + "\n");
    }
    QLocalServer server;
    QList<QLocalSocket *> clients;
    QLocalSocket *frameSocket = nullptr;
    QStringList methods;
};

class StreamDeckTests : public QObject {
    Q_OBJECT

private slots:
    void clientSubscribesAndLoadsState() {
        QTemporaryDir dir;
        FakeDaemon fake(dir.filePath("control.sock"));
        FrameStore frames;
        DaemonClient client(&frames, nullptr, dir.filePath("control.sock"));
        QTRY_VERIFY(client.connected());
        QTRY_COMPARE(client.state().value("devices").toList().size(), 1);
        QVERIFY(fake.methods.contains("subscribe"));
        QTRY_VERIFY(fake.methods.contains("subscribeFrames"));
    }

    void clientRoutesRepliesAndErrors() {
        QTemporaryDir dir;
        FakeDaemon fake(dir.filePath("control.sock"));
        FrameStore frames;
        DaemonClient client(&frames, nullptr, dir.filePath("control.sock"));
        QTRY_VERIFY(client.connected());
        QVariant got;
        QString err = "unset";
        client.request("ping", {{"x", 1}}, [&](const QVariant &r, const QString &e) { got = r; err = e; });
        QTRY_VERIFY(got.isValid());
        QCOMPARE(err, QString());
        QCOMPARE(got.toMap().value("echo").toMap().value("x").toInt(), 1);
        QString failure;
        client.request("fail", {}, [&](const QVariant &, const QString &e) { failure = e; });
        QTRY_COMPARE(failure, QString("nope"));
    }

    void clientForwardsEventsAndState() {
        QTemporaryDir dir;
        FakeDaemon fake(dir.filePath("control.sock"));
        FrameStore frames;
        DaemonClient client(&frames, nullptr, dir.filePath("control.sock"));
        QTRY_VERIFY(client.connected());
        QTRY_VERIFY(!fake.clients.isEmpty());
        QTRY_COMPARE(client.state().value("devices").toList().size(), 1);
        QSignalSpy events(&client, &DaemonClient::event);
        fake.push(fake.clients.first(), QJsonObject{{"event", "state"}, {"data", QJsonObject{{"devices", QJsonArray{}}}}});
        QTRY_COMPARE(events.size(), 1);
        QCOMPARE(events.at(0).at(0).toString(), QString("state"));
        QCOMPARE(client.state().value("devices").toList().size(), 0);
    }

    void framesLandInTheImageProvider() {
        QTemporaryDir dir;
        FakeDaemon fake(dir.filePath("control.sock"));
        FrameStore frames;
        DaemonClient client(&frames, nullptr, dir.filePath("control.sock"));
        QTRY_VERIFY(fake.frameSocket != nullptr);
        QImage red(8, 8, QImage::Format_ARGB32);
        red.fill(Qt::red);
        QByteArray png;
        QBuffer buf(&png);
        red.save(&buf, "PNG");
        QSignalSpy changed(&client, &DaemonClient::frameChanged);
        fake.push(fake.frameSocket, QJsonObject{{"event", "frame"}, {"data", QJsonObject{{"device", "virtual-1"}, {"target", "key"}, {"index", 4}, {"png", QString::fromLatin1(png.toBase64())}}}});
        QTRY_COMPARE(changed.size(), 1);
        QCOMPARE(client.frameRevision("virtual-1/key-4"), 1);
        QSize size;
        const QImage got = frames.requestImage("virtual-1/key-4?1", &size, QSize());
        QCOMPARE(size, QSize(8, 8));
        QCOMPARE(QColor(got.pixel(3, 3)), QColor(Qt::red));
    }

    void clientReconnectsAfterTheServiceRestarts() {
        QTemporaryDir dir;
        const QString path = dir.filePath("control.sock");
        FrameStore frames;
        DaemonClient client(&frames, nullptr, path);
        QVERIFY(!client.connected());
        FakeDaemon fake(path);
        QTRY_VERIFY_WITH_TIMEOUT(client.connected(), 5000);
    }

    void keyNamesReadLikeOmarchy() {
        AppUtil util;
        QCOMPARE(util.keyName(Qt::Key_4, Qt::MetaModifier | Qt::ShiftModifier), QString("Super + Shift + 4"));
        QCOMPARE(util.keyName(Qt::Key_Control, Qt::ControlModifier), QString("Ctrl"));
    }

    void themeFallsBack() {
        QTemporaryDir dir;
        Theme theme(dir.path());
        QCOMPARE(theme.accent(), QString("#FFD60A"));
        QCOMPARE(theme.accentForeground(), QString("black"));
    }

    void themeReadsAccent() {
        QTemporaryDir dir;
        QDir(dir.path()).mkpath("theme");
        QFile colors(dir.path() + "/theme/colors.toml");
        QVERIFY(colors.open(QIODevice::WriteOnly));
        colors.write("accent = \"#112233\"\n");
        colors.close();
        Theme theme(dir.path());
        QCOMPARE(theme.accent(), QString("#112233"));
        QCOMPARE(theme.accentForeground(), QString("white"));
    }

    void translatorFallsBackToEnglish() {
        QTemporaryDir dir;
        Translator translator(dir.path() + "/language", dir.path());
        QCOMPARE(translator.language(), QString("en"));
        QVERIFY(translator.translate("Main", "Save").isNull());
    }

    void translatorPicksPluralForms() {
        Translator translator;
        QCOMPARE(translator.translate("Main", "%n page(s)", nullptr, 1), QString("%n page"));
        QCOMPARE(translator.translate("Main", "%n page(s)", nullptr, 3), QString("%n pages"));
    }
};

QTEST_MAIN(StreamDeckTests)
#include "streamdeck_tests.moc"
