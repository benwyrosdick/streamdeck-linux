import QtQuick
import QtQuick.Controls
import QtQuick.Controls.Material

// Records one key combination in the original app's format: a Qt key code
// plus modifier flags. The Linux key code is stored beside it so the combo
// replays the physical key whatever keymap is active.
Rectangle {
    id: field
    property var value: null          // a Hotkeys entry
    property string placeholder: qsTr("Click and press keys")
    signal changed(var entry)

    readonly property int unknownKey: 33554431
    readonly property bool hasValue: !!value && ((value.QTKeyCode !== undefined && value.QTKeyCode !== unknownKey) || (value.KeyModifiers || 0) > 0)
    property bool recording: false
    property int heldModifiers: 0

    implicitWidth: 260
    implicitHeight: 40
    radius: Ui.small
    color: Ui.well
    border.color: recording ? Ui.accent : (activeFocus ? Ui.dim : Ui.border)
    border.width: recording ? 2 : 1
    activeFocusOnTab: true

    function describe(v) {
        if (!v) return ""
        let mods = 0
        if (v.KeyShift) mods |= Qt.ShiftModifier
        if (v.KeyCtrl) mods |= Qt.ControlModifier
        if (v.KeyOption) mods |= Qt.AltModifier
        if (v.KeyCmd) mods |= Qt.MetaModifier
        return util.keyName(v.QTKeyCode === unknownKey ? 0 : v.QTKeyCode, mods)
    }

    function entry(key, mods, scan) {
        const shift = !!(mods & Qt.ShiftModifier), ctrl = !!(mods & Qt.ControlModifier), alt = !!(mods & Qt.AltModifier), meta = !!(mods & Qt.MetaModifier)
        return {
            KeyCmd: meta, KeyCtrl: ctrl, KeyOption: alt, KeyShift: shift,
            KeyModifiers: (shift ? 1 : 0) | (ctrl ? 2 : 0) | (alt ? 4 : 0) | (meta ? 8 : 0),
            QTKeyCode: key || unknownKey,
            NativeCode: -1, VKeyCode: -1,
            // XKB key codes are evdev codes plus 8.
            LinuxKeyCode: scan > 8 ? scan - 8 : -1
        }
    }

    Label {
        anchors.fill: parent
        anchors.leftMargin: 12
        anchors.rightMargin: 34
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
        text: field.recording ? (field.heldModifiers ? util.keyName(0, field.heldModifiers) + " + …" : qsTr("Press a key combination…")) : (field.hasValue ? field.describe(field.value) : field.placeholder)
        color: field.recording || field.hasValue ? Ui.text : Ui.faint
        textFormat: Text.PlainText
    }

    ToolButton {
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        visible: field.hasValue && !field.recording
        text: "×"
        onClicked: field.changed(field.entry(0, 0, 0))
    }

    MouseArea {
        anchors.fill: parent
        anchors.rightMargin: 34
        onClicked: {
            field.recording = true
            field.heldModifiers = 0
            field.forceActiveFocus()
        }
    }

    onActiveFocusChanged: if (!activeFocus) recording = false

    // Claim every key while recording, so window shortcuts do not fire.
    Keys.onShortcutOverride: function (e) { e.accepted = field.recording }
    Keys.onPressed: function (e) {
        if (!field.recording) {
            if (e.key === Qt.Key_Return || e.key === Qt.Key_Space) { field.recording = true; e.accepted = true }
            return
        }
        e.accepted = true
        const isModifier = [Qt.Key_Shift, Qt.Key_Control, Qt.Key_Meta, Qt.Key_Alt, Qt.Key_Super_L, Qt.Key_Super_R, Qt.Key_AltGr].indexOf(e.key) >= 0
        if (isModifier) {
            field.heldModifiers = e.modifiers
            return
        }
        if (e.key === Qt.Key_Escape && e.modifiers === Qt.NoModifier) {
            field.recording = false
            return
        }
        // Qt reports Shift+1 as "!"; the original stores the unshifted key with Shift.
        field.changed(field.entry(e.key, e.modifiers, e.nativeScanCode))
        field.recording = false
    }
    Keys.onReleased: function (e) {
        if (!field.recording) return
        e.accepted = true
        // Releasing modifiers that were pressed alone records them as the combo.
        if (field.heldModifiers && e.modifiers === Qt.NoModifier) {
            field.changed(field.entry(0, field.heldModifiers, 0))
            field.recording = false
        }
    }
}
