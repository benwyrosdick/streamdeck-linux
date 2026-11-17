import QtQuick
import QtQuick.Controls
import QtQuick.Controls.Material
import QtQuick.Layouts
import QtWebEngine

// Hosts a plugin's own property inspector: an HTML page that connects to
// streamdeckd's SDK socket once we call connectElgatoStreamDeckSocket().
Rectangle {
    id: host
    property string context: ""
    property var info: null
    property string error: ""
    // Elgato's inspector stylesheets are drawn for this grey.
    color: "#2d2d2d"

    onContextChanged: reload()
    Component.onCompleted: reload()

    function reload() {
        info = null
        error = ""
        if (!context) return
        const ctx = context
        Store.call("piInfo", { context: ctx }, function (r, err) {
            if (ctx !== host.context) return true
            if (err) { host.error = err; return true }
            // The page calls nothing itself; like the original app we call its
            // connectElgatoStreamDeckSocket() once it has loaded, retrying
            // briefly for inspectors that define it in deferred scripts.
            const call = "connectElgatoStreamDeckSocket(" + r.port + ", " + JSON.stringify(r.uuid) + ", "
                + JSON.stringify(r.registerEvent) + ", " + JSON.stringify(JSON.stringify(r.info)) + ", "
                + JSON.stringify(JSON.stringify(r.actionInfo)) + ")"
            const source = "(function () { var tries = 0; function go() {"
                + " if (typeof connectElgatoStreamDeckSocket === 'function') { " + call + "; return; }"
                + " if (++tries < 100) setTimeout(go, 50); }"
                + " if (document.readyState === 'complete') go(); else window.addEventListener('load', go); })();"
            web.userScripts.collection = [{ name: "streamdeck-connect", sourceCode: source, injectionPoint: WebEngineScript.DocumentReady, worldId: WebEngineScript.MainWorld, runsOnSubFrames: false }]
            host.info = r
        })
    }

    Connections {
        target: Store
        // A plugin that finishes starting after the inspector opened needs a fresh connection.
        function onRevisionChanged() {
            if (host.info && !host.info.pluginRunning) host.reload()
        }
    }

    WebEngineView {
        id: web
        anchors.fill: parent
        visible: !!host.info && !!host.info.path
        backgroundColor: "transparent"
        url: host.info && host.info.path ? util.fileUrl(host.info.path) : "about:blank"
        settings.localContentCanAccessRemoteUrls: true
        settings.localContentCanAccessFileUrls: true
        settings.javascriptCanAccessClipboard: true
        settings.javascriptCanPaste: true
        settings.focusOnNavigationEnabled: false

        // Links in inspectors open in the browser, never inside the editor.
        onNewWindowRequested: function (request) {
            util.openUrl(request.requestedUrl.toString())
        }
        onNavigationRequested: function (request) {
            const u = request.url.toString()
            if (u.startsWith("http://") || u.startsWith("https://")) {
                request.reject()
                util.openUrl(u)
            }
        }
        onJavaScriptConsoleMessage: function (level, message, line, source) {
            if (level >= 1) console.warn("[inspector] " + message + " (" + source + ":" + line + ")")
        }
    }

    ColumnLayout {
        anchors.centerIn: parent
        width: parent.width - 60
        visible: !web.visible
        spacing: 8
        Label {
            Layout.fillWidth: true
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
            textFormat: Text.PlainText
            color: host.error || (host.info && host.info.unsupported) ? Ui.danger : Ui.dim
            text: host.error ? host.error
                : !host.info ? qsTr("Loading…")
                : host.info.unsupported ? qsTr("This plugin cannot run on Linux: %1").arg(host.info.unsupported)
                : qsTr("This action has no settings.")
        }
        Label {
            Layout.fillWidth: true
            horizontalAlignment: Text.AlignHCenter
            visible: !!host.info && !host.info.pluginRunning && !host.info.unsupported
            wrapMode: Text.WordWrap
            color: Ui.faint
            text: qsTr("The plugin is starting; settings take effect once it connects.")
        }
    }
}
