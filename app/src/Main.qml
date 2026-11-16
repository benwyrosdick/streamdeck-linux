import QtQuick
import QtQuick.Controls
import QtQuick.Controls.Material
import QtQuick.Layouts
import "qml"

ApplicationWindow {
    id: win
    width: 1240
    height: 820
    minimumWidth: 900
    minimumHeight: 600
    visible: true
    title: Store.device ? qsTr("Stream Deck — %1").arg(Store.device.name) : qsTr("Stream Deck")
    color: Ui.window

    Material.theme: Material.Dark
    Material.accent: theme.accent
    Material.roundedScale: theme.radius
    Material.background: Ui.panel

    property bool helpVisible: false

    function inField() {
        const f = win.activeFocusItem
        return f && (f instanceof TextInput || f instanceof TextEdit || (f.hasOwnProperty("cursorPosition")))
    }

    Shortcut { sequence: "?"; context: Qt.ApplicationShortcut; onActivated: win.helpVisible = !win.helpVisible }
    Shortcut { sequence: "Q"; context: Qt.ApplicationShortcut; enabled: !win.inField(); onActivated: Qt.quit() }
    Shortcut { sequence: "Space"; context: Qt.ApplicationShortcut; enabled: !win.inField() && Store.selected !== null; onActivated: Store.press(Store.selected) }
    Shortcut { sequences: [StandardKey.Undo]; context: Qt.ApplicationShortcut; enabled: !win.inField(); onActivated: Store.undo() }
    Shortcut { sequences: [StandardKey.Redo, "Ctrl+Shift+Z"]; context: Qt.ApplicationShortcut; enabled: !win.inField(); onActivated: Store.redo() }
    Shortcut { sequences: [StandardKey.Copy]; context: Qt.ApplicationShortcut; enabled: !win.inField(); onActivated: Store.copySelected() }
    Shortcut { sequences: [StandardKey.Cut]; context: Qt.ApplicationShortcut; enabled: !win.inField(); onActivated: { Store.copySelected(); Store.removeAction(Store.selected) } }
    Shortcut { sequences: [StandardKey.Paste]; context: Qt.ApplicationShortcut; enabled: !win.inField(); onActivated: Store.pasteTo(Store.selected) }
    Shortcut { sequences: [StandardKey.Delete, "Backspace"]; context: Qt.ApplicationShortcut; enabled: !win.inField() && Store.selected !== null; onActivated: Store.removeAction(Store.selected) }
    Shortcut { sequence: "PgDown"; context: Qt.ApplicationShortcut; enabled: !win.inField(); onActivated: Store.call("pageStep", { device: Store.deviceId, delta: 1 }) }
    Shortcut { sequence: "PgUp"; context: Qt.ApplicationShortcut; enabled: !win.inField(); onActivated: Store.call("pageStep", { device: Store.deviceId, delta: -1 }) }
    Shortcut { sequence: "Ctrl+,"; context: Qt.ApplicationShortcut; onActivated: prefs.open() }
    Shortcut { sequence: "Ctrl+F"; context: Qt.ApplicationShortcut; onActivated: actionList.focusSearch() }
    Shortcut {
        sequence: "Escape"
        context: Qt.ApplicationShortcut
        onActivated: {
            if (win.helpVisible) win.helpVisible = false
            else if (Store.selected && Store.selected.path && Store.selected.path.length) Store.selected = { controller: Store.selected.controller, position: Store.selected.position, path: Store.selected.path.slice(0, -1) }
            else if (Store.selected) Store.selected = null
            else if (Store.device && Store.device.inFolder) Store.call("closeFolder", { device: Store.deviceId })
        }
    }

    Item {
        anchors.fill: parent
        LayoutMirroring.enabled: Qt.application.layoutDirection === Qt.RightToLeft
        LayoutMirroring.childrenInherit: true

        ColumnLayout {
            anchors.fill: parent
            spacing: 0
            visible: Store.connected

            TopBar {
                Layout.fillWidth: true
                onPreferencesRequested: prefs.open()
            }

            SplitView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                orientation: Qt.Horizontal

                handle: Rectangle {
                    implicitWidth: 1
                    color: Ui.border
                }

                SplitView {
                    SplitView.fillWidth: true
                    SplitView.minimumWidth: 480
                    orientation: Qt.Vertical
                    handle: Rectangle {
                        implicitHeight: 1
                        color: Ui.border
                    }

                    ColumnLayout {
                        SplitView.fillHeight: true
                        SplitView.minimumHeight: 240
                        spacing: 0

                        PageBar {
                            Layout.fillWidth: true
                        }

                        DeviceCanvas {
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                        }
                    }

                    Inspector {
                        SplitView.preferredHeight: Math.max(300, win.height * 0.38)
                        SplitView.minimumHeight: 240
                    }
                }

                ActionList {
                    id: actionList
                    SplitView.preferredWidth: 300
                    SplitView.minimumWidth: 220
                }
            }
        }

        // Shown until the service answers.
        ColumnLayout {
            anchors.centerIn: parent
            visible: !Store.connected
            spacing: 14

            Label {
                Layout.alignment: Qt.AlignHCenter
                text: qsTr("The Stream Deck service is not running")
                font.pixelSize: 20
                color: Ui.text
            }
            Label {
                Layout.alignment: Qt.AlignHCenter
                Layout.maximumWidth: 460
                wrapMode: Text.WordWrap
                horizontalAlignment: Text.AlignHCenter
                text: qsTr("streamdeckd talks to your Stream Deck and runs plugins. Start it here, or enable it with systemctl --user enable --now streamdeckd.")
                color: Ui.dim
                textFormat: Text.PlainText
            }
            Button {
                Layout.alignment: Qt.AlignHCenter
                text: qsTr("Start service")
                highlighted: true
                onClicked: daemon.startService()
            }
        }

        // Plugins, profiles and icon packs can be dropped anywhere on the window.
        DropArea {
            anchors.fill: parent
            keys: ["text/uri-list"]
            onDropped: function (drop) {
                for (const u of drop.urls) {
                    const p = util.localPath(u)
                    if (p.endsWith(".streamDeckPlugin")) Store.call("installPlugin", { path: p }, function (r, e) { if (!e) Store.toast(qsTr("Installed %1").arg(r), false) })
                    else if (p.endsWith(".streamDeckProfile")) Store.call("importProfile", { path: p, device: Store.deviceId }, function (r, e) { if (!e) Store.toast(qsTr("Profile imported"), false) })
                    else if (p.endsWith(".streamDeckIconPack")) Store.call("installIconPack", { path: p }, function (r, e) { if (!e) Store.toast(qsTr("Icon pack installed"), false) })
                }
                drop.accept()
            }
        }

        Toasts {
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.bottom: parent.bottom
            anchors.bottomMargin: 24
        }

        HelpOverlay {
            anchors.fill: parent
            visible: win.helpVisible
            onCloseRequested: win.helpVisible = false
        }
    }

    Preferences {
        id: prefs
    }
}
