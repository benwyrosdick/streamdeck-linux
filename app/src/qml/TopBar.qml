import QtQuick
import QtQuick.Controls
import QtQuick.Controls.Material
import QtQuick.Layouts

Rectangle {
    id: bar
    implicitHeight: 56
    color: Ui.panel

    signal preferencesRequested()

    Rectangle {
        anchors.bottom: parent.bottom
        width: parent.width
        height: 1
        color: Ui.border
    }

    RowLayout {
        anchors.fill: parent
        anchors.leftMargin: 14
        anchors.rightMargin: 10
        spacing: 10

        ComboBox {
            id: deviceBox
            Layout.preferredWidth: 230
            model: Store.devices
            textRole: "name"
            currentIndex: Store.devices.findIndex(d => d.id === Store.deviceId)
            onActivated: function (i) { Store.deviceId = Store.devices[i].id }
            displayText: Store.device ? Store.device.name + (Store.device.virtual ? "" : "  ·  " + Store.device.modelName) : qsTr("No device")
            ToolTip.visible: hovered
            ToolTip.text: qsTr("Device")
        }

        ComboBox {
            id: profileBox
            Layout.preferredWidth: 260
            model: Store.profiles
            textRole: "name"
            currentIndex: Store.profiles.findIndex(p => p.id === Store.profileId)
            onActivated: function (i) { Store.selectProfile(Store.profiles[i].id) }
            displayText: Store.device ? (Store.device.profileName || "") + (Store.device.smartProfile ? "  ·  " + qsTr("smart") : "") : ""
            delegate: ItemDelegate {
                required property var modelData
                required property int index
                width: profileBox.width
                contentItem: RowLayout {
                    Label {
                        Layout.fillWidth: true
                        text: modelData.name
                        textFormat: Text.PlainText
                        elide: Text.ElideRight
                        color: modelData.id === Store.profileId ? Ui.accent : Ui.text
                    }
                    Label {
                        visible: !!modelData.app
                        text: modelData.app || ""
                        textFormat: Text.PlainText
                        color: Ui.faint
                        font.pixelSize: 11
                    }
                }
                onClicked: { Store.selectProfile(modelData.id); profileBox.popup.close() }
            }
        }

        ToolButton {
            text: "⋯"
            font.pixelSize: 18
            onClicked: profileMenu.popup()
            ToolTip.visible: hovered
            ToolTip.text: qsTr("Profile actions")

            Menu {
                id: profileMenu
                MenuItem { text: qsTr("New profile"); onTriggered: nameDialog.ask(qsTr("New profile"), qsTr("Profile"), function (n) { Store.call("createProfile", { device: Store.deviceId, name: n }, function (id) { if (id) Store.selectProfile(id) }) }) }
                MenuItem { text: qsTr("Rename…"); enabled: !!Store.profileId; onTriggered: nameDialog.ask(qsTr("Rename profile"), Store.device.profileName, function (n) { Store.call("renameProfile", { profile: Store.profileId, name: n }) }) }
                MenuItem { text: qsTr("Duplicate"); enabled: !!Store.profileId; onTriggered: Store.call("duplicateProfile", { profile: Store.profileId }) }
                MenuSeparator {}
                MenuItem { text: qsTr("Switch with an application…"); enabled: !!Store.profileId; onTriggered: smartPicker.open() }
                MenuItem {
                    text: qsTr("Stop switching automatically")
                    enabled: !!(Store.profile && Store.profile.manifest && Store.profile.manifest.AppIdentifier)
                    onTriggered: Store.call("setProfileApp", { profile: Store.profileId, app: "" })
                }
                MenuSeparator {}
                MenuItem {
                    text: qsTr("Import…")
                    onTriggered: filePicker.open(qsTr("Import profile"), [{ name: qsTr("Stream Deck profiles"), patterns: ["*.streamDeckProfile"] }], false, true, function (paths) {
                        for (const p of paths) Store.call("importProfile", { path: p, device: Store.deviceId }, function (r, e) { if (!e) Store.toast(qsTr("Profile imported"), false) })
                    })
                }
                MenuItem {
                    text: qsTr("Export…")
                    enabled: !!Store.profileId
                    onTriggered: filePicker.save(qsTr("Export profile"), (Store.device.profileName || "Profile") + ".streamDeckProfile", [{ name: qsTr("Stream Deck profiles"), patterns: ["*.streamDeckProfile"] }], function (paths) {
                        if (paths.length) Store.call("exportProfile", { profile: Store.profileId, path: paths[0] }, function (r, e) { if (!e) Store.toast(qsTr("Exported to %1").arg(r), false) })
                    })
                }
                MenuSeparator {}
                MenuItem {
                    text: qsTr("Delete profile")
                    enabled: Store.profiles.length > 1
                    onTriggered: confirmDelete.open()
                }
            }
        }

        Item { Layout.fillWidth: true }

        Label {
            visible: Store.device && Store.device.asleep
            text: qsTr("Asleep")
            color: Ui.dim
        }

        RowLayout {
            spacing: 4
            visible: !!Store.device
            Label {
                text: "☀"
                color: Ui.dim
                font.pixelSize: 16
            }
            Slider {
                id: brightness
                Layout.preferredWidth: 130
                from: 0
                to: 100
                stepSize: 5
                value: Store.device ? Store.device.brightness : 60
                onMoved: Store.call("setBrightness", { device: Store.deviceId, value: Math.round(value) })
                ToolTip.visible: hovered || pressed
                ToolTip.text: qsTr("Brightness %1%").arg(Math.round(value))
            }
        }

        ToolButton {
            text: "⚙"
            font.pixelSize: 18
            onClicked: bar.preferencesRequested()
            ToolTip.visible: hovered
            ToolTip.text: qsTr("Preferences (Ctrl+,)")
        }
    }

    Dialog {
        id: nameDialog
        property var done: null
        anchors.centerIn: Overlay.overlay
        modal: true
        width: 360
        standardButtons: Dialog.Ok | Dialog.Cancel
        function ask(title, initial, cb) {
            nameDialog.title = title
            nameField.text = initial || ""
            done = cb
            open()
            nameField.forceActiveFocus()
            nameField.selectAll()
        }
        TextField {
            id: nameField
            implicitWidth: 320
            onAccepted: nameDialog.accept()
        }
        onAccepted: if (done && nameField.text.trim().length) done(nameField.text.trim())
    }

    Dialog {
        id: confirmDelete
        anchors.centerIn: Overlay.overlay
        modal: true
        width: 400
        title: qsTr("Delete profile?")
        standardButtons: Dialog.Yes | Dialog.Cancel
        Label {
            text: qsTr("“%1” and all its pages and folders will be deleted. Automatic backups keep a copy.").arg(Store.device ? Store.device.profileName : "")
            wrapMode: Text.WordWrap
            width: confirmDelete.availableWidth
            textFormat: Text.PlainText
        }
        onAccepted: Store.call("deleteProfile", { profile: Store.profileId })
    }

    AppPicker {
        id: smartPicker
        title: qsTr("Switch to this profile when this app is focused")
        onPicked: function (app) {
            // Hyprland reports the window class, which StartupWMClass names when set.
            const ident = app.wmClass && app.wmClass.length ? app.wmClass : app.id
            Store.call("setProfileApp", { profile: Store.profileId, app: ident }, function (r, e) {
                if (!e) Store.toast(qsTr("Profile follows %1").arg(app.name), false)
            })
        }
    }
}
