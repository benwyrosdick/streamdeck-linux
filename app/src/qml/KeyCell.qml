import QtQuick
import QtQuick.Controls
import QtQuick.Controls.Material

// One key (or dial) of the device as the editor shows it. The face is the
// device's real rendered frame, so plugin titles and images appear exactly
// as on the hardware.
Item {
    id: cell
    property string controller: "Keypad"
    property string position: "0,0"
    property int frameIndex: 0
    property string frameTarget: "key"
    property int revision: 0
    property real radius: Ui.small + 4
    property bool showFrame: true

    readonly property var sel: ({ controller: controller, position: position, path: [] })
    readonly property var action: { Store.revision; return Store.actionAt(sel) }
    readonly property var def: action ? Store.definition(action.UUID) : null
    readonly property bool selected: Store.sameSel(Store.selected, sel) || (Store.selected && Store.selected.controller === controller && Store.selected.position === position)
    readonly property bool isFolder: !!action && action.UUID === "com.elgato.streamdeck.profile.openchild"

    Connections {
        target: Store
        function onFrameChanged(device, target, index, rev) {
            if (device === Store.deviceId && target === cell.frameTarget && index === cell.frameIndex)
                cell.revision = rev
        }
    }
    Component.onCompleted: revision = daemon.frameRevision(Store.deviceId + "/" + frameTarget + "-" + frameIndex)

    Rectangle {
        id: face
        anchors.fill: parent
        radius: cell.radius
        color: "#000"
        border.width: cell.selected ? 3 : (hover.containsMouse || drop.containsDrag ? 2 : 1)
        border.color: cell.selected ? Ui.accent : (drop.containsDrag ? Ui.accent : (hover.containsMouse ? "#4a4a52" : "#26262b"))
        clip: true

        Image {
            anchors.fill: parent
            anchors.margins: 3
            visible: cell.showFrame
            source: Store.deviceId ? "image://frames/" + Store.deviceId + "/" + cell.frameTarget + "-" + cell.frameIndex + "?" + cell.revision : ""
            cache: false
            smooth: true
            mipmap: true
            fillMode: Image.PreserveAspectCrop
        }

        Label {
            anchors.centerIn: parent
            visible: !cell.action && hover.containsMouse && !drop.containsDrag
            text: "+"
            color: Ui.faint
            font.pixelSize: cell.height * 0.3
        }
    }

    MouseArea {
        id: hover
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        drag.target: cell.action ? ghost : null
        drag.threshold: 6
        cursorShape: drag.active ? Qt.ClosedHandCursor : Qt.PointingHandCursor

        onPressed: function (mouse) {
            if (mouse.button === Qt.LeftButton) {
                ghost.grab(mouse)
                Store.selected = cell.sel
            }
        }
        onReleased: function (mouse) { ghost.finish(mouse) }
        onClicked: function (mouse) {
            if (mouse.button === Qt.RightButton) {
                Store.selected = cell.sel
                menu.popup()
            }
        }
        onDoubleClicked: {
            // Opening a folder in the editor opens it on the device too.
            if (cell.isFolder) Store.press(cell.sel)
        }

        ToolTip.visible: containsMouse && !!cell.def && !drag.active
        ToolTip.delay: 700
        ToolTip.text: cell.def ? cell.def.name + (cell.isFolder ? " — " + qsTr("double-click to open") : "") : ""
    }

    DragGhost {
        id: ghost
        handle: hover
        kindKey: "key"
        payload: ({ kind: "key", sel: cell.sel })
        icon: cell.def && cell.def.icon ? util.fileUrl(cell.def.icon) : ""
    }

    DropArea {
        id: drop
        anchors.fill: parent
        keys: ["action", "key", "text/uri-list"]
        onDropped: function (ev) {
            if (ev.hasUrls) {
                const p = util.localPath(ev.urls[0])
                if (/\.(png|jpe?g|gif|svg|webp)$/i.test(p) && cell.action) {
                    Store.setImage(cell.sel, cell.action.State || 0, p)
                    ev.accept()
                }
                return
            }
            const src = ev.source
            if (!src || !src.payload) return
            if (src.payload.kind === "action") {
                const d = Store.definition(src.payload.uuid)
                if (d && d.controllers.indexOf(cell.controller) < 0) {
                    Store.toast(qsTr("%1 cannot go on a %2.").arg(d.name).arg(cell.controller === "Encoder" ? qsTr("dial") : qsTr("key")), true)
                    return
                }
                Store.placeAction(cell.sel, src.payload.uuid)
            } else if (src.payload.kind === "key") {
                if (src.payload.sel.controller !== cell.controller) {
                    Store.toast(qsTr("Keys and dials cannot swap places."), true)
                    return
                }
                Store.moveAction(src.payload.sel, cell.sel, src.copy)
            }
            ev.accept()
        }
    }

    Menu {
        id: menu
        MenuItem { text: qsTr("Press"); enabled: !!cell.action; onTriggered: Store.press(cell.sel) }
        MenuItem { text: qsTr("Open folder"); visible: cell.isFolder; height: visible ? implicitHeight : 0; onTriggered: Store.press(cell.sel) }
        MenuSeparator {}
        MenuItem { text: qsTr("Copy"); enabled: !!cell.action; onTriggered: { Store.selected = cell.sel; Store.copySelected() } }
        MenuItem { text: qsTr("Cut"); enabled: !!cell.action; onTriggered: { Store.selected = cell.sel; Store.copySelected(); Store.removeAction(cell.sel) } }
        MenuItem { text: qsTr("Paste"); enabled: !!Store.clipboard; onTriggered: Store.pasteTo(cell.sel) }
        MenuSeparator {}
        MenuItem {
            text: qsTr("Set image from file…")
            enabled: !!cell.action
            onTriggered: filePicker.open(qsTr("Choose an image"), [{ name: qsTr("Images"), patterns: ["*.png", "*.jpg", "*.jpeg", "*.gif", "*.svg", "*.webp"] }], false, false, function (paths) {
                if (paths.length) Store.setImage(cell.sel, cell.action.State || 0, paths[0])
            })
        }
        MenuItem { text: qsTr("Reset image"); enabled: !!cell.action; onTriggered: Store.setImage(cell.sel, cell.action.State || 0, "") }
        MenuSeparator {}
        MenuItem { text: qsTr("Clear"); enabled: !!cell.action; onTriggered: Store.removeAction(cell.sel) }
    }
}
