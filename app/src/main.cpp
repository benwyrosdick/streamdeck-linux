// streamdeck: the Stream Deck editor for Linux.
//
//   streamdeck                 open the editor
//   streamdeck vsd [device]    open the Virtual Stream Deck window
//   streamdeck open FILE|URL   import a profile, plugin or icon pack; pass a streamdeck:// link
//   streamdeck plugin-host …   run an HTML plugin (started by streamdeckd)

#include <QGuiApplication>
#include <QIcon>
#include <QJsonDocument>
#include <QLocalSocket>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickStyle>
#include <QQuickWindow>
#include <QTimer>
#include <QFileInfo>
#include <QUrl>
#include <QtWebEngineQuick/QtWebEngineQuick>
#include <cstdio>

#include "apputil.h"
#include "daemonclient.h"
#include "pluginhost.h"
#include "portalfilepicker.h"
#include "theme.h"
#include "translator.h"

static const char *kVersion = "0.1.0";

// `open` works without a window: it hands the file to the service and exits.
static int openFile(const QString &target) {
    QString socketPath = qEnvironmentVariable("STREAMDECK_RUNTIME_DIR");
    if (socketPath.isEmpty())
        socketPath = qEnvironmentVariable("XDG_RUNTIME_DIR") + "/streamdeck-linux";
    socketPath += "/control.sock";
    QLocalSocket s;
    s.connectToServer(socketPath);
    if (!s.waitForConnected(2000)) {
        fprintf(stderr, "streamdeckd is not running\n");
        return 1;
    }
    QString method;
    QVariantMap params;
    if (target.startsWith("streamdeck://")) {
        method = "deepLink";
        params["url"] = target;
    } else {
        const QString path = QFileInfo(target).absoluteFilePath();
        params["path"] = path;
        if (path.endsWith(".streamDeckPlugin") || path.endsWith(".sdPlugin"))
            method = "installPlugin";
        else if (path.endsWith(".streamDeckIconPack") || path.endsWith(".sdIconPack"))
            method = "installIconPack";
        else
            method = "importProfile";
    }
    QVariantMap req{{"id", 1}, {"method", method}, {"params", params}};
    s.write(QJsonDocument::fromVariant(req).toJson(QJsonDocument::Compact) + "\n");
    s.flush();
    if (!s.waitForReadyRead(15000)) {
        fprintf(stderr, "no reply from streamdeckd\n");
        return 1;
    }
    const QVariantMap reply = QJsonDocument::fromJson(s.readLine()).toVariant().toMap();
    if (reply.contains("error")) {
        fprintf(stderr, "%s\n", qPrintable(reply.value("error").toString()));
        return 1;
    }
    printf("%s\n", qPrintable(QJsonDocument::fromVariant(reply.value("result")).toJson(QJsonDocument::Compact)));
    return 0;
}

int main(int argc, char *argv[]) {
    const QString mode = argc > 1 ? QString::fromLocal8Bit(argv[1]) : QString();
    if (mode == "--version") {
        printf("streamdeck %s\n", kVersion);
        return 0;
    }
    if (mode == "--help" || mode == "-h") {
        printf("usage: streamdeck [vsd [DEVICE] | open FILE|URL | plugin-host PAGE ARGS…]\n");
        return 0;
    }
    if (mode == "open") {
        QCoreApplication app(argc, argv);
        if (argc < 3) {
            fprintf(stderr, "usage: streamdeck open FILE|URL\n");
            return 2;
        }
        return openFile(QString::fromLocal8Bit(argv[2]));
    }
    // A .desktop handler passes the file directly; treat that like `open`.
    if (!mode.isEmpty() && mode != "vsd" && mode != "plugin-host" && (mode.startsWith("streamdeck://") || QFileInfo::exists(mode))) {
        QCoreApplication app(argc, argv);
        return openFile(mode);
    }

    QtWebEngineQuick::initialize();
    QGuiApplication app(argc, argv);
    app.setApplicationName("streamdeck");
    app.setApplicationVersion(kVersion);
    app.setOrganizationName("streamdeck-linux");

    if (mode == "plugin-host") {
        app.setQuitOnLastWindowClosed(false);
        PluginHost host(app.arguments().mid(2));
        if (!host.valid()) {
            fprintf(stderr, "usage: streamdeck plugin-host PAGE -port N -pluginUUID U -registerEvent E -info JSON\n");
            return 2;
        }
        return app.exec();
    }

    app.setDesktopFileName(mode == "vsd" ? "streamdeck-vsd" : "streamdeck");
    app.setWindowIcon(QIcon::fromTheme("streamdeck", QIcon(":/icons/streamdeck.svg")));
    QQuickStyle::setStyle("Material");

    Translator translator;
    app.installTranslator(&translator);
    Theme theme;
    auto *frames = new FrameStore;
    DaemonClient daemon(frames);
    PortalFilePicker picker;
    AppUtil util;

    QQmlApplicationEngine engine;
    engine.addImageProvider("frames", frames);
    daemon.setEngine(&engine);
    picker.setEngine(&engine);
    QObject::connect(&translator, &Translator::changed, &engine, &QQmlEngine::retranslate);
    auto *ctx = engine.rootContext();
    ctx->setContextProperty("translator", &translator);
    ctx->setContextProperty("theme", &theme);
    ctx->setContextProperty("daemon", &daemon);
    ctx->setContextProperty("filePicker", &picker);
    ctx->setContextProperty("util", &util);
    ctx->setContextProperty("startDevice", argc > 2 ? QString::fromLocal8Bit(argv[2]) : QString());
    engine.load(QUrl(mode == "vsd" ? "qrc:/VirtualDeckWindow.qml" : "qrc:/Main.qml"));
    if (engine.rootObjects().isEmpty())
        return 1;

    // For tests and README screenshots: render, save the window, and quit.
    const QString shot = qEnvironmentVariable("STREAMDECK_SCREENSHOT");
    if (!shot.isEmpty()) {
        auto *window = qobject_cast<QQuickWindow *>(engine.rootObjects().first());
        const int delay = qEnvironmentVariableIntValue("STREAMDECK_SCREENSHOT_DELAY");
        QTimer::singleShot(delay > 0 ? delay : 2500, &app, [window, shot] {
            if (window)
                window->grabWindow().save(shot);
            QCoreApplication::quit();
        });
    }
    return app.exec();
}
