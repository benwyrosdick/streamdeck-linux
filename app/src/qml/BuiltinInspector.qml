import QtQuick
import QtQuick.Controls
import QtQuick.Controls.Material
import QtQuick.Layouts

// Settings forms for the built-in actions. Settings keys follow the original
// app wherever its profiles show them, so profiles move between the two.
ScrollView {
    id: root
    property var sel
    property var action
    property var def
    property int stateIndex: 0

    readonly property var s: action && action.Settings ? action.Settings : ({})
    readonly property string uuid: action ? action.UUID : ""
    readonly property bool isDial: !!sel && sel.controller === "Encoder"

    function set(patch) { Store.updateSettings(sel, patch) }

    contentWidth: availableWidth
    clip: true

    Loader {
        width: root.availableWidth
        sourceComponent: {
            switch (root.uuid) {
            case "com.elgato.streamdeck.system.hotkey": return hotkey
            case "com.elgato.streamdeck.system.hotkeyswitch": return hotkeySwitch
            case "com.elgato.streamdeck.system.open": return openForm
            case "com.elgato.streamdeck.system.openapp": return openApp
            case "com.elgato.streamdeck.system.close": return closeApp
            case "com.elgato.streamdeck.system.website": return website
            case "com.elgato.streamdeck.system.text": return textForm
            case "com.elgato.streamdeck.system.multimedia": return multimedia
            case "com.elgato.streamdeck.system.keybrightness": return brightness
            case "com.elgato.streamdeck.system.mouse-wheel": return mouseWheel
            case "com.elgato.streamdeck.system.sleep": return sleepForm
            case "com.elgato.streamdeck.system.timer": return timer
            case "com.elgato.streamdeck.soundboard.playaudio": return playAudio
            case "com.elgato.streamdeck.multiactions.routine":
            case "com.elgato.streamdeck.multiactions.random": return multi
            case "com.elgato.streamdeck.multiactions.routine2": return multiSwitch
            case "com.elgato.streamdeck.multiactions.delay": return delay
            case "com.elgato.streamdeck.page.goto": return gotoPage
            case "com.elgato.streamdeck.profile.openchild": return folder
            case "com.elgato.streamdeck.profile.rotate": return switchProfile
            case "com.elgato.streamdeck.keys.logic": return keyLogic
            case "com.elgato.streamdeck.keys.stack": return actionWheel
            case "com.elgato.streamdeck.keys.adaptor": return actionTrigger
            case "com.elgato.streamdeck.dial.stack": return dialStack
            case "com.elgato.streamdeck.system.digitaltime": return digitalTime
            default: return info
            }
        }
    }

    // ------------------------------------------------------------------

    component Form: ColumnLayout {
        spacing: 8
        Item { Layout.preferredHeight: 4 }
    }

    component Hint: Label {
        Layout.fillWidth: true
        Layout.leftMargin: 14
        Layout.rightMargin: 14
        wrapMode: Text.WordWrap
        color: Ui.faint
        font.pixelSize: 12
        textFormat: Text.PlainText
    }

    component Row2: RowLayout {
        Layout.fillWidth: true
        Layout.leftMargin: 14
        Layout.rightMargin: 14
        spacing: 10
    }

    Component {
        id: info
        Form {
            Hint {
                text: root.def ? (root.def.tooltip || root.def.name) + "\n\n" + qsTr("This action has no settings.") : ""
            }
        }
    }

    function hotkeys() {
        const list = (s.Hotkeys || []).slice()
        while (list.length < 4) list.push({ KeyCmd: false, KeyCtrl: false, KeyModifiers: 0, KeyOption: false, KeyShift: false, NativeCode: -1, QTKeyCode: 33554431, VKeyCode: -1 })
        return list
    }
    function setHotkey(i, entry) {
        const list = hotkeys()
        list[i] = entry
        set({ Hotkeys: list, Coalesce: true })
    }

    Component {
        id: hotkey
        Form {
            Repeater {
                model: root.isDial ? [qsTr("Rotate left"), qsTr("Rotate right"), qsTr("Push"), qsTr("Touch")] : [qsTr("Key combination")]
                delegate: ColumnLayout {
                    required property string modelData
                    required property int index
                    FieldLabel { text: modelData }
                    HotkeyField {
                        Layout.leftMargin: 14
                        value: root.hotkeys()[index]
                        onChanged: function (e) { root.setHotkey(index, e) }
                    }
                }
            }
            Hint {
                text: root.isDial ? qsTr("Each tick of the dial sends its combination once.") : qsTr("The keys stay held while the Stream Deck key is held, so push-to-talk shortcuts work. Cmd/Super is the Super key.")
            }
        }
    }

    Component {
        id: hotkeySwitch
        Form {
            Repeater {
                model: [qsTr("State 1"), qsTr("State 2")]
                delegate: ColumnLayout {
                    required property string modelData
                    required property int index
                    FieldLabel { text: modelData }
                    HotkeyField {
                        Layout.leftMargin: 14
                        value: root.hotkeys()[index]
                        onChanged: function (e) { root.setHotkey(index, e) }
                    }
                }
            }
            Hint { text: qsTr("Each press sends the current state's combination, then switches state.") }
        }
    }

    Component {
        id: openForm
        Form {
            FieldLabel { text: qsTr("File, folder, program or command") }
            Row2 {
                TextField {
                    Layout.fillWidth: true
                    text: root.s.path || ""
                    placeholderText: qsTr("~/Documents or firefox --private-window")
                    onEditingFinished: if (text !== (root.s.path || "")) root.set({ path: text })
                }
                Button { text: qsTr("File…"); onClicked: filePicker.open(qsTr("Choose a file"), [], false, false, function (p) { if (p.length) root.set({ path: "\"" + p[0] + "\"" }) }) }
                Button { text: qsTr("Folder…"); onClicked: filePicker.open(qsTr("Choose a folder"), [], true, false, function (p) { if (p.length) root.set({ path: "\"" + p[0] + "\"" }) }) }
                Button { text: qsTr("App…"); onClicked: apps.open() }
            }
            Hint { text: qsTr("Files and folders open in their default application. Anything else runs as a command.") }
            AppPicker {
                id: apps
                onPicked: function (app) { root.set({ path: "\"" + app.path + "\"" }) }
            }
        }
    }

    component AppChooser: Row2 {
        id: chooser
        property var current: null
        Image {
            Layout.preferredWidth: 32
            Layout.preferredHeight: 32
            source: chooser.current && chooser.current.iconPath ? util.fileUrl(chooser.current.iconPath) : ""
            sourceSize: Qt.size(64, 64)
        }
        Label {
            Layout.fillWidth: true
            text: chooser.current ? chooser.current.name : (root.s.app_name || qsTr("No application chosen"))
            textFormat: Text.PlainText
            color: chooser.current ? Ui.text : Ui.dim
            elide: Text.ElideRight
        }
        Button {
            text: qsTr("Choose…")
            onClicked: picker.open()
        }
        AppPicker {
            id: picker
            onPicked: function (app) { root.set({ bundle_path: app.path, app_name: app.name }) }
        }
        Component.onCompleted: Store.loadApplications(function (list) {
            const key = root.s.bundle_path || root.s.app_name || ""
            chooser.current = list.find(a => a.path === key || a.id === key || a.name === key) || null
        })
    }

    Component {
        id: openApp
        Form {
            FieldLabel { text: qsTr("Application") }
            AppChooser {}
            Row2 {
                Switch {
                    text: qsTr("Bring to front if already running")
                    checked: root.s.bring_to_front !== false
                    onToggled: root.set({ bring_to_front: checked })
                }
            }
            FieldLabel { text: qsTr("When held") }
            Row2 {
                ComboBox {
                    Layout.preferredWidth: 220
                    readonly property var values: ["quit", "force_quit", "none"]
                    model: [qsTr("Quit the app"), qsTr("Force quit the app"), qsTr("Do nothing")]
                    currentIndex: Math.max(0, values.indexOf(root.s.long_press || "quit"))
                    onActivated: function (i) { root.set({ long_press: values[i] }) }
                }
            }
            Hint { text: qsTr("A green dot on the key shows the app is running.") }
        }
    }

    Component {
        id: closeApp
        Form {
            FieldLabel { text: qsTr("Close every window of") }
            AppChooser {}
            Hint { text: qsTr("Hold the key to force quit instead.") }
        }
    }

    Component {
        id: website
        Form {
            FieldLabel { text: qsTr("URL") }
            Row2 {
                TextField {
                    Layout.fillWidth: true
                    text: root.s.path || ""
                    placeholderText: "https://"
                    onEditingFinished: if (text !== (root.s.path || "")) root.set({ path: text })
                }
            }
            Row2 {
                RadioButton {
                    text: qsTr("Open in browser")
                    checked: root.s.openInBrowser !== false
                    onClicked: root.set({ openInBrowser: true })
                }
                RadioButton {
                    text: qsTr("Access in background")
                    checked: root.s.openInBrowser === false
                    onClicked: root.set({ openInBrowser: false })
                }
            }
            Hint { text: qsTr("In the background the URL is requested without opening a window; the key shows a check mark when it succeeds. Useful for webhooks.") }
        }
    }

    Component {
        id: textForm
        Form {
            FieldLabel { text: qsTr("Text") }
            ScrollView {
                Layout.fillWidth: true
                Layout.leftMargin: 14
                Layout.rightMargin: 14
                Layout.preferredHeight: 90
                TextArea {
                    text: root.s.pastedText || ""
                    wrapMode: TextEdit.Wrap
                    onActiveFocusChanged: if (!activeFocus && text !== (root.s.pastedText || "")) root.set({ pastedText: text })
                }
            }
            Row2 {
                Switch {
                    text: qsTr("Type it instead of pasting")
                    checked: !!root.s.isTypingMode
                    onToggled: root.set({ isTypingMode: checked })
                }
                Switch {
                    text: qsTr("Press Enter afterwards")
                    checked: !!root.s.isSendingEnter
                    onToggled: root.set({ isSendingEnter: checked })
                }
            }
            Hint { text: qsTr("Pasting puts the text on the clipboard and sends the paste shortcut (Ctrl+Shift+V in terminals).") }
        }
    }

    Component {
        id: multimedia
        Form {
            FieldLabel { text: qsTr("Action") }
            Row2 {
                ComboBox {
                    Layout.preferredWidth: 300
                    // Index order matches the original app's actionIdx.
                    readonly property var items: [
                        [0, qsTr("Previous track")], [1, qsTr("Play / Pause")], [2, qsTr("Next track")], [3, qsTr("Stop")],
                        [4, qsTr("Mute")], [5, qsTr("Volume up")], [6, qsTr("Volume down")],
                        [7, qsTr("Display brightness up")], [8, qsTr("Display brightness down")],
                        [11, qsTr("Eject")], [12, qsTr("Switch display mode")], [13, qsTr("Fast forward")], [14, qsTr("Rewind")],
                        [15, qsTr("Keyboard backlight up")], [16, qsTr("Keyboard backlight down")], [17, qsTr("Keyboard backlight on/off")],
                        [18, qsTr("Volume (dial)")], [19, qsTr("Display brightness (dial)")], [21, qsTr("Keyboard backlight (dial)")], [22, qsTr("Player (dial)")]
                    ]
                    model: items.map(i => i[1])
                    currentIndex: Math.max(0, items.findIndex(i => i[0] === (root.s.actionIdx === undefined ? (root.isDial ? 18 : 1) : root.s.actionIdx)))
                    onActivated: function (i) { root.set({ actionIdx: items[i][0] }) }
                }
            }
            Hint { text: root.isDial ? qsTr("Turn to adjust; push to mute, toggle or play/pause.") : qsTr("Sends the same media key a keyboard would, so Omarchy shows its usual on-screen display.") }
        }
    }

    Component {
        id: brightness
        Form {
            FieldLabel { text: qsTr("Brightness") }
            Row2 {
                ComboBox {
                    Layout.preferredWidth: 220
                    model: [qsTr("Increase"), qsTr("Decrease"), qsTr("Maximum"), qsTr("High"), qsTr("Medium"), qsTr("Low"), qsTr("Minimum")]
                    currentIndex: root.s.actionIdx || 0
                    onActivated: function (i) { root.set({ actionIdx: i }) }
                }
            }
            Hint { text: root.isDial ? qsTr("Turn to adjust; push to switch between dim and full.") : "" }
        }
    }

    Component {
        id: mouseWheel
        Form {
            FieldLabel { text: qsTr("Direction") }
            Row2 {
                ComboBox {
                    Layout.preferredWidth: 200
                    model: [qsTr("Vertical"), qsTr("Horizontal")]
                    currentIndex: root.s.direction === "horizontal" ? 1 : 0
                    onActivated: function (i) { root.set({ direction: i === 1 ? "horizontal" : "vertical" }) }
                }
                Switch {
                    text: qsTr("Reverse")
                    checked: !!root.s.invert
                    onToggled: root.set({ invert: checked })
                }
            }
            FieldLabel { text: qsTr("Lines per tick") }
            Row2 {
                SpinBox {
                    from: 1
                    to: 20
                    value: root.s.step || 1
                    onValueModified: root.set({ step: value })
                }
            }
        }
    }

    Component {
        id: sleepForm
        Form {
            FieldLabel { text: qsTr("Device") }
            Row2 {
                ComboBox {
                    Layout.preferredWidth: 260
                    model: [qsTr("This Stream Deck")].concat(Store.devices.map(d => d.name))
                    currentIndex: Math.max(0, Store.devices.findIndex(d => d.id === root.s.DeviceUUID) + 1)
                    onActivated: function (i) { root.set({ DeviceUUID: i === 0 ? "" : Store.devices[i - 1].id }) }
                }
            }
            Hint { text: qsTr("The keys go dark until the next press.") }
        }
    }

    Component {
        id: timer
        Form {
            readonly property int total: root.s.duration === undefined ? 60 : root.s.duration
            FieldLabel { text: qsTr("Duration") }
            Row2 {
                SpinBox { id: h; from: 0; to: 23; value: Math.floor(parent.parent.total / 3600); onValueModified: root.set({ duration: h.value * 3600 + m.value * 60 + sec.value }) }
                Label { text: qsTr("h"); color: Ui.dim }
                SpinBox { id: m; from: 0; to: 59; value: Math.floor(parent.parent.total / 60) % 60; onValueModified: root.set({ duration: h.value * 3600 + m.value * 60 + sec.value }) }
                Label { text: qsTr("min"); color: Ui.dim }
                SpinBox { id: sec; from: 0; to: 59; value: parent.parent.total % 60; onValueModified: root.set({ duration: h.value * 3600 + m.value * 60 + sec.value }) }
                Label { text: qsTr("s"); color: Ui.dim }
            }
            FieldLabel { text: qsTr("Sound when done") }
            Row2 {
                TextField {
                    Layout.fillWidth: true
                    text: root.s.sound || ""
                    placeholderText: qsTr("Default chime")
                    onEditingFinished: root.set({ sound: text })
                }
                Button { text: qsTr("Choose…"); onClicked: filePicker.open(qsTr("Choose a sound"), [{ name: qsTr("Audio"), patterns: ["*.mp3", "*.wav", "*.ogg", "*.oga", "*.flac", "*.m4a"] }], false, false, function (p) { if (p.length) root.set({ sound: p[0] }) }) }
            }
            Hint { text: qsTr("Press to start or pause. Hold to reset.") }
        }
    }

    Component {
        id: playAudio
        Form {
            FieldLabel { text: qsTr("Sound file, or a folder to play a random file from") }
            Row2 {
                TextField {
                    Layout.fillWidth: true
                    text: root.s.path || ""
                    onEditingFinished: if (text !== (root.s.path || "")) root.set({ path: text })
                }
                Button { text: qsTr("File…"); onClicked: filePicker.open(qsTr("Choose a sound"), [{ name: qsTr("Audio"), patterns: ["*.mp3", "*.wav", "*.ogg", "*.oga", "*.flac", "*.m4a", "*.opus"] }], false, false, function (p) { if (p.length) root.set({ path: p[0] }) }) }
                Button { text: qsTr("Folder…"); onClicked: filePicker.open(qsTr("Choose a folder"), [], true, false, function (p) { if (p.length) root.set({ path: p[0] }) }) }
            }
            FieldLabel { text: qsTr("Playback") }
            Row2 {
                ComboBox {
                    Layout.preferredWidth: 220
                    model: [qsTr("Play / Stop"), qsTr("Play / Overlap"), qsTr("Play / Restart"), qsTr("Loop / Stop"), qsTr("Press & Hold")]
                    currentIndex: root.s.actionType || 0
                    onActivated: function (i) { root.set({ actionType: i }) }
                }
            }
            FieldLabel { text: qsTr("Volume") }
            Row2 {
                Slider {
                    Layout.preferredWidth: 220
                    from: 0
                    to: 100
                    stepSize: 1
                    value: root.s.volume === undefined ? 100 : Number(root.s.volume)
                    onPressedChanged: if (!pressed) root.set({ volume: Math.round(value) })
                }
                Label { text: Math.round(root.s.volume === undefined ? 100 : Number(root.s.volume)) + "%"; color: Ui.dim }
            }
            FieldLabel { text: qsTr("Fade") }
            Row2 {
                ComboBox {
                    Layout.preferredWidth: 160
                    model: [qsTr("None"), qsTr("Fade in"), qsTr("Fade out"), qsTr("Fade in and out")]
                    currentIndex: root.s.fadeType || 0
                    onActivated: function (i) { root.set({ fadeType: i }) }
                }
                SpinBox {
                    from: 0
                    to: 10000
                    stepSize: 100
                    editable: true
                    value: root.s.fadeLen || 0
                    onValueModified: root.set({ fadeLen: value })
                }
                Label { text: qsTr("ms"); color: Ui.dim }
            }
        }
    }

    Component {
        id: multi
        Form {
            ChildActions {
                Layout.fillWidth: true
                parentSel: root.sel
                parentAction: root.action
                routine: 0
                title: root.uuid.endsWith("random") ? qsTr("One of these runs at random") : qsTr("These run in order")
            }
        }
    }

    Component {
        id: multiSwitch
        Form {
            ChildActions { Layout.fillWidth: true; parentSel: root.sel; parentAction: root.action; routine: 0; title: qsTr("First press (state 1)") }
            ChildActions { Layout.fillWidth: true; parentSel: root.sel; parentAction: root.action; routine: 1; title: qsTr("Second press (state 2)") }
        }
    }

    Component {
        id: delay
        Form {
            FieldLabel { text: qsTr("Wait") }
            Row2 {
                SpinBox {
                    from: 10
                    to: 600000
                    stepSize: 100
                    editable: true
                    value: root.s.delay || 500
                    onValueModified: root.set({ delay: value })
                }
                Label { text: qsTr("milliseconds"); color: Ui.dim }
            }
        }
    }

    Component {
        id: gotoPage
        Form {
            FieldLabel { text: qsTr("Page") }
            Row2 {
                SpinBox {
                    from: 1
                    to: Math.max(1, Store.device ? Store.device.pageCount : 1)
                    value: root.s.PageIndex || 1
                    onValueModified: root.set({ PageIndex: value })
                }
            }
        }
    }

    Component {
        id: folder
        Form {
            Hint { text: qsTr("Put keys inside the folder by opening it. A Parent Folder key in the top-left corner leads back.") }
            Row2 {
                Button {
                    text: qsTr("Open folder")
                    highlighted: true
                    onClicked: Store.press(root.sel)
                }
            }
        }
    }

    Component {
        id: switchProfile
        Form {
            readonly property string targetDevice: root.s.DeviceUUID || Store.deviceId
            property var targetProfiles: []
            Component.onCompleted: Store.call("listProfiles", { device: targetDevice }, function (r) { if (r) targetProfiles = r })
            onTargetDeviceChanged: Store.call("listProfiles", { device: targetDevice }, function (r) { if (r) targetProfiles = r })

            FieldLabel { text: qsTr("Device") }
            Row2 {
                ComboBox {
                    Layout.preferredWidth: 260
                    model: Store.devices.map(d => d.name)
                    currentIndex: Math.max(0, Store.devices.findIndex(d => d.id === parent.parent.targetDevice))
                    onActivated: function (i) { root.set({ DeviceUUID: Store.devices[i].id, ProfileUUID: "" }) }
                }
            }
            FieldLabel { text: qsTr("Profile") }
            Row2 {
                ComboBox {
                    Layout.preferredWidth: 260
                    model: [qsTr("Next profile")].concat(parent.parent.targetProfiles.map(p => p.name))
                    currentIndex: Math.max(0, parent.parent.targetProfiles.findIndex(p => p.id === root.s.ProfileUUID) + 1)
                    onActivated: function (i) { root.set({ ProfileUUID: i === 0 ? "" : parent.parent.targetProfiles[i - 1].id }) }
                }
            }
            FieldLabel { text: qsTr("Page (0 keeps the profile's own)") }
            Row2 {
                SpinBox {
                    from: 0
                    to: 50
                    value: root.s.PageIndex || 0
                    onValueModified: root.set({ PageIndex: value })
                }
            }
        }
    }

    Component {
        id: keyLogic
        Form {
            ChildActions { Layout.fillWidth: true; parentSel: root.sel; parentAction: root.action; routine: 0; title: qsTr("Press") }
            ChildActions { Layout.fillWidth: true; parentSel: root.sel; parentAction: root.action; routine: 1; title: qsTr("Double press") }
            ChildActions { Layout.fillWidth: true; parentSel: root.sel; parentAction: root.action; routine: 2; title: qsTr("Hold") }
            FieldLabel { text: qsTr("Double press within") }
            Row2 {
                SpinBox {
                    from: 100
                    to: 1000
                    stepSize: 50
                    value: root.s.doublePressMs || 300
                    onValueModified: root.set({ doublePressMs: value })
                }
                Label { text: qsTr("ms"); color: Ui.dim }
            }
            Hint { text: qsTr("A hold is half a second.") }
        }
    }

    Component {
        id: actionWheel
        Form {
            ChildActions { Layout.fillWidth: true; parentSel: root.sel; parentAction: root.action; routine: 0; title: qsTr("Turn to choose, push to run"); controllerFilter: "Keypad" }
        }
    }

    Component {
        id: actionTrigger
        Form {
            ChildActions { Layout.fillWidth: true; parentSel: root.sel; parentAction: root.action; routine: 0; title: qsTr("Rotate left"); controllerFilter: "Keypad" }
            ChildActions { Layout.fillWidth: true; parentSel: root.sel; parentAction: root.action; routine: 1; title: qsTr("Rotate right"); controllerFilter: "Keypad" }
            ChildActions { Layout.fillWidth: true; parentSel: root.sel; parentAction: root.action; routine: 2; title: qsTr("Push"); controllerFilter: "Keypad" }
            ChildActions { Layout.fillWidth: true; parentSel: root.sel; parentAction: root.action; routine: 3; title: qsTr("Touch"); controllerFilter: "Keypad" }
        }
    }

    Component {
        id: dialStack
        Form {
            ChildActions { Layout.fillWidth: true; parentSel: root.sel; parentAction: root.action; routine: 0; title: qsTr("Push the dial to move to the next one"); controllerFilter: "Encoder" }
        }
    }

    Component {
        id: digitalTime
        Form {
            Row2 {
                Switch {
                    text: qsTr("24-hour clock")
                    checked: root.s["24HourFormat"] !== false
                    onToggled: root.set({ "24HourFormat": checked })
                }
            }
        }
    }
}
