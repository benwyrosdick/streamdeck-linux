import QtQuick
import QtQuick.Controls
import QtQuick.Controls.Material
import QtQuick.Layouts

Dialog {
    id: prefs
    title: qsTr("Preferences")
    modal: true
    anchors.centerIn: Overlay.overlay
    width: Math.min(860, Overlay.overlay ? Overlay.overlay.width - 60 : 860)
    height: Math.min(640, Overlay.overlay ? Overlay.overlay.height - 60 : 640)
    standardButtons: Dialog.Close
    padding: 0

    property var settings: ({})
    property var backups: []
    property var iconPacks: []
    property string pluginLog: ""

    onOpened: refresh()

    function refresh() {
        Store.call("getSettings", {}, function (r) { if (r) prefs.settings = r })
        Store.call("listBackups", {}, function (r) { if (r) prefs.backups = r })
        Store.call("listIconPacks", {}, function (r) { if (r) prefs.iconPacks = r })
        Store.refreshPlugins()
        Store.refreshProfiles()
    }

    function setSetting(key, value) {
        const patch = {}
        patch[key] = value
        Store.call("setSettings", patch, function () { prefs.refresh() })
    }

    contentItem: ColumnLayout {
        spacing: 0

        TabBar {
            id: tabs
            Layout.fillWidth: true
            TabButton { text: qsTr("General") }
            TabButton { text: qsTr("Devices") }
            TabButton { text: qsTr("Profiles") }
            TabButton { text: qsTr("Plugins") }
            TabButton { text: qsTr("Icon packs") }
            TabButton { text: qsTr("Backups") }
        }

        StackLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.margins: 18
            currentIndex: tabs.currentIndex

            // General ----------------------------------------------------
            ColumnLayout {
                spacing: 12
                Switch {
                    text: qsTr("Switch profiles automatically with the focused application")
                    checked: prefs.settings.smartProfiles !== false
                    onToggled: prefs.setSetting("smartProfiles", checked)
                }
                Switch {
                    text: qsTr("Show keys shrinking while pressed")
                    checked: prefs.settings.pressFeedback !== false
                    onToggled: prefs.setSetting("pressFeedback", checked)
                }
                RowLayout {
                    Switch {
                        text: qsTr("Back up profiles automatically, keeping")
                        checked: prefs.settings.autoBackup !== false
                        onToggled: prefs.setSetting("autoBackup", checked)
                    }
                    SpinBox {
                        from: 1
                        to: 100
                        value: prefs.settings.backupsToKeep || 10
                        onValueModified: prefs.setSetting("backupsToKeep", value)
                    }
                    Label { text: qsTr("backups"); color: Ui.dim }
                }
                RowLayout {
                    Label { text: qsTr("Plugin language"); color: Ui.dim }
                    ComboBox {
                        readonly property var codes: ["en", "de", "es", "fr", "it", "ja", "ko", "pt", "zh_CN", "zh_TW"]
                        model: ["English", "Deutsch", "Español", "Français", "Italiano", "日本語", "한국어", "Português", "简体中文", "繁體中文"]
                        currentIndex: Math.max(0, codes.indexOf(prefs.settings.language || "en"))
                        onActivated: function (i) { prefs.setSetting("language", codes[i]) }
                    }
                }
                Label {
                    Layout.fillWidth: true
                    wrapMode: Text.WordWrap
                    color: Ui.faint
                    text: qsTr("Plugins connect to ws://127.0.0.1:%1. Profiles, plugins and backups live in %2.").arg(prefs.settings.sdkPort || "").arg(util.dataPath())
                    textFormat: Text.PlainText
                }
                Button {
                    text: qsTr("Open data folder")
                    onClicked: util.openUrl(util.fileUrl(util.dataPath()))
                }
                Item { Layout.fillHeight: true }
            }

            // Devices ----------------------------------------------------
            ScrollView {
                clip: true
                ColumnLayout {
                    width: parent.width
                    spacing: 16
                    Repeater {
                        model: Store.devices
                        delegate: Frame {
                            id: devFrame
                            required property var modelData
                            Layout.fillWidth: true
                            background: Rectangle { color: Ui.raised; radius: Ui.radius; border.color: Ui.border }
                            GridLayout {
                                anchors.fill: parent
                                columns: 2
                                columnSpacing: 14
                                rowSpacing: 8
                                Label { text: qsTr("Name"); color: Ui.dim }
                                TextField {
                                    Layout.fillWidth: true
                                    text: devFrame.modelData.name
                                    onEditingFinished: if (text !== devFrame.modelData.name) Store.call("renameDevice", { device: devFrame.modelData.id, name: text })
                                }
                                Label { text: qsTr("Model"); color: Ui.dim }
                                Label {
                                    text: devFrame.modelData.modelName + (devFrame.modelData.virtual ? "" : "  ·  " + qsTr("serial %1").arg(devFrame.modelData.serial) + (devFrame.modelData.firmware ? "  ·  " + qsTr("firmware %1").arg(devFrame.modelData.firmware) : ""))
                                    textFormat: Text.PlainText
                                }
                                Label { text: qsTr("Brightness"); color: Ui.dim }
                                Slider {
                                    Layout.fillWidth: true
                                    from: 0; to: 100; stepSize: 5
                                    value: devFrame.modelData.brightness
                                    onMoved: Store.call("setBrightness", { device: devFrame.modelData.id, value: Math.round(value) })
                                }
                                Label { text: qsTr("Sleep after"); color: Ui.dim }
                                ComboBox {
                                    readonly property var minutes: [0, 1, 5, 10, 15, 30, 60, 120]
                                    model: [qsTr("Never"), qsTr("1 minute"), qsTr("5 minutes"), qsTr("10 minutes"), qsTr("15 minutes"), qsTr("30 minutes"), qsTr("1 hour"), qsTr("2 hours")]
                                    currentIndex: Math.max(0, minutes.indexOf(devFrame.modelData.sleepMinutes))
                                    onActivated: function (i) { Store.call("setSleep", { device: devFrame.modelData.id, minutes: minutes[i] }) }
                                }
                                Item { visible: devFrame.modelData.virtual; Layout.preferredHeight: 1 }
                                RowLayout {
                                    visible: devFrame.modelData.virtual
                                    Button {
                                        text: qsTr("Open window")
                                        onClicked: util.launch("streamdeck", ["vsd", devFrame.modelData.id])
                                    }
                                    Button {
                                        text: qsTr("Remove")
                                        onClicked: Store.call("removeVirtualDeck", { id: devFrame.modelData.id })
                                    }
                                }
                            }
                        }
                    }

                    Frame {
                        Layout.fillWidth: true
                        background: Rectangle { color: "transparent"; radius: Ui.radius; border.color: Ui.border }
                        RowLayout {
                            anchors.fill: parent
                            Label { text: qsTr("Add a Virtual Stream Deck with"); color: Ui.dim }
                            SpinBox { id: vCols; from: 1; to: 16; value: 8 }
                            Label { text: qsTr("columns and"); color: Ui.dim }
                            SpinBox { id: vRows; from: 1; to: 16; value: 4 }
                            Label { text: qsTr("rows"); color: Ui.dim }
                            Item { Layout.fillWidth: true }
                            Button {
                                text: qsTr("Add")
                                highlighted: true
                                onClicked: Store.call("addVirtualDeck", { columns: vCols.value, rows: vRows.value })
                            }
                        }
                    }
                }
            }

            // Profiles ---------------------------------------------------
            ColumnLayout {
                spacing: 8
                Label {
                    text: Store.device ? qsTr("Profiles for %1").arg(Store.device.name) : ""
                    color: Ui.dim
                    textFormat: Text.PlainText
                }
                ListView {
                    id: profileList
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    model: Store.profiles
                    delegate: ItemDelegate {
                        required property var modelData
                        width: profileList.width
                        height: 48
                        highlighted: modelData.id === Store.profileId
                        onClicked: Store.selectProfile(modelData.id)
                        contentItem: RowLayout {
                            Label {
                                Layout.fillWidth: true
                                text: modelData.name
                                textFormat: Text.PlainText
                                elide: Text.ElideRight
                            }
                            Label {
                                text: modelData.app ? qsTr("follows %1").arg(modelData.app) : ""
                                textFormat: Text.PlainText
                                color: Ui.faint
                            }
                            Label {
                                text: qsTr("%n page(s)", "", modelData.pages.length)
                                color: Ui.faint
                            }
                            ToolButton {
                                text: qsTr("Export")
                                onClicked: filePicker.save(qsTr("Export profile"), modelData.name + ".streamDeckProfile", [{ name: qsTr("Stream Deck profiles"), patterns: ["*.streamDeckProfile"] }], function (paths) {
                                    if (paths.length) Store.call("exportProfile", { profile: modelData.id, path: paths[0] }, function (r, e) { if (!e) Store.toast(qsTr("Exported"), false) })
                                })
                            }
                            ToolButton {
                                text: qsTr("Duplicate")
                                onClicked: Store.call("duplicateProfile", { profile: modelData.id })
                            }
                            ToolButton {
                                text: qsTr("Delete")
                                enabled: Store.profiles.length > 1
                                onClicked: Store.call("deleteProfile", { profile: modelData.id })
                            }
                        }
                    }
                }
                RowLayout {
                    Button {
                        text: qsTr("New profile")
                        onClicked: Store.call("createProfile", { device: Store.deviceId, name: qsTr("New Profile") })
                    }
                    Button {
                        text: qsTr("Import…")
                        onClicked: filePicker.open(qsTr("Import profile"), [{ name: qsTr("Stream Deck profiles"), patterns: ["*.streamDeckProfile"] }], false, true, function (paths) {
                            for (const p of paths) Store.call("importProfile", { path: p, device: Store.deviceId })
                        })
                    }
                }
            }

            // Plugins ----------------------------------------------------
            SplitView {
                orientation: Qt.Vertical
                ListView {
                    id: pluginList
                    SplitView.fillHeight: true
                    clip: true
                    model: Store.plugins.filter(p => !p.builtin)
                    delegate: ItemDelegate {
                        required property var modelData
                        width: pluginList.width
                        height: 60
                        contentItem: RowLayout {
                            spacing: 12
                            Image {
                                Layout.preferredWidth: 32
                                Layout.preferredHeight: 32
                                source: modelData.icon ? util.fileUrl(modelData.icon + (modelData.icon.endsWith(".png") || modelData.icon.endsWith(".svg") ? "" : ".png")) : ""
                                sourceSize: Qt.size(64, 64)
                            }
                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 2
                                Label {
                                    text: modelData.name + "  " + modelData.version
                                    textFormat: Text.PlainText
                                    font.bold: true
                                }
                                Label {
                                    Layout.fillWidth: true
                                    elide: Text.ElideRight
                                    textFormat: Text.PlainText
                                    color: modelData.unsupported ? Ui.danger : (modelData.running ? "#3ecf6b" : Ui.dim)
                                    text: modelData.unsupported ? modelData.unsupported : (modelData.running ? qsTr("Running · %1").arg(modelData.author) : qsTr("Not running · %1").arg(modelData.author))
                                }
                            }
                            ToolButton { text: qsTr("Log"); onClicked: Store.call("pluginLog", { uuid: modelData.uuid }, function (r) { prefs.pluginLog = r || qsTr("(empty)") }) }
                            ToolButton { text: qsTr("Restart"); onClicked: Store.call("restartPlugin", { uuid: modelData.uuid }) }
                            ToolButton { text: qsTr("Uninstall"); onClicked: Store.call("uninstallPlugin", { uuid: modelData.uuid }) }
                        }
                    }
                    footer: Button {
                        text: qsTr("Install plugin…")
                        onClicked: filePicker.open(qsTr("Install a Stream Deck plugin"), [{ name: qsTr("Stream Deck plugins"), patterns: ["*.streamDeckPlugin"] }], false, false, function (p) {
                            if (p.length) Store.installPlugin(p[0])
                        })
                    }
                    Label {
                        anchors.centerIn: parent
                        visible: pluginList.count === 0
                        color: Ui.dim
                        text: qsTr("No plugins installed. Plugins built with Elgato's Node.js SDK run as they are; drop a .streamDeckPlugin file on the window to install one.")
                        wrapMode: Text.WordWrap
                        width: parent.width - 60
                        horizontalAlignment: Text.AlignHCenter
                    }
                }
                ScrollView {
                    SplitView.preferredHeight: prefs.pluginLog ? 180 : 0
                    visible: !!prefs.pluginLog
                    TextArea {
                        readOnly: true
                        text: prefs.pluginLog
                        font.family: "monospace"
                        font.pixelSize: 11
                        wrapMode: TextEdit.NoWrap
                    }
                }
            }

            // Icon packs -------------------------------------------------
            ColumnLayout {
                ListView {
                    id: packList
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    model: prefs.iconPacks
                    delegate: ItemDelegate {
                        required property var modelData
                        width: packList.width
                        height: 52
                        contentItem: RowLayout {
                            Image {
                                Layout.preferredWidth: 32
                                Layout.preferredHeight: 32
                                source: modelData.icon ? util.fileUrl(modelData.icon) : ""
                            }
                            Label {
                                Layout.fillWidth: true
                                text: (modelData.name || modelData.id) + " — " + qsTr("%n icon(s)", "", modelData.icons.length)
                                textFormat: Text.PlainText
                            }
                            ToolButton { text: qsTr("Remove"); onClicked: Store.call("removeIconPack", { id: modelData.id }, function () { prefs.refresh() }) }
                        }
                    }
                }
                Button {
                    text: qsTr("Install icon pack…")
                    onClicked: filePicker.open(qsTr("Install an icon pack"), [{ name: qsTr("Icon packs"), patterns: ["*.streamDeckIconPack"] }], false, false, function (p) {
                        if (p.length) Store.call("installIconPack", { path: p[0] }, function () { prefs.refresh() })
                    })
                }
            }

            // Backups ----------------------------------------------------
            ColumnLayout {
                Label {
                    Layout.fillWidth: true
                    wrapMode: Text.WordWrap
                    color: Ui.dim
                    text: qsTr("A backup holds every profile and the settings. Restoring replaces all profiles; the current ones are backed up first.")
                }
                ListView {
                    id: backupList
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    model: prefs.backups
                    delegate: ItemDelegate {
                        required property var modelData
                        width: backupList.width
                        height: 44
                        contentItem: RowLayout {
                            Label {
                                Layout.fillWidth: true
                                text: new Date(modelData.modified * 1000).toLocaleString(Qt.locale(), Locale.ShortFormat) + "  ·  " + modelData.name
                                textFormat: Text.PlainText
                                elide: Text.ElideRight
                            }
                            Label {
                                text: Math.round(modelData.size / 1024) + " kB"
                                color: Ui.faint
                            }
                            ToolButton {
                                text: qsTr("Restore")
                                onClicked: Store.call("restoreBackup", { path: modelData.path }, function (r, e) { if (!e) { Store.toast(qsTr("Profiles restored"), false); prefs.refresh() } })
                            }
                        }
                    }
                }
                RowLayout {
                    Button {
                        text: qsTr("Back up now")
                        highlighted: true
                        onClicked: Store.call("backup", {}, function () { prefs.refresh() })
                    }
                    Button {
                        text: qsTr("Restore from file…")
                        onClicked: filePicker.open(qsTr("Restore a backup"), [{ name: qsTr("Stream Deck backups"), patterns: ["*.streamDeckProfilesBackup"] }], false, false, function (p) {
                            if (p.length) Store.call("restoreBackup", { path: p[0] }, function () { prefs.refresh() })
                        })
                    }
                }
            }
        }
    }
}
