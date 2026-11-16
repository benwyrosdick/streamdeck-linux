import QtQuick
import QtQuick.Controls
import QtQuick.Controls.Material
import QtQuick.Layouts

// The steps of one routine of a container action. Actions are dropped in
// from the list; selecting a step opens its own settings.
ColumnLayout {
    id: box
    property var parentSel
    property var parentAction
    property int routine: 0
    property string title: ""
    property string controllerFilter: "Keypad"

    readonly property var children: parentAction && parentAction.Actions && parentAction.Actions[routine] ? parentAction.Actions[routine].Actions : []
    readonly property bool isMulti: !!parentAction && parentAction.UUID.indexOf("multiactions") >= 0

    spacing: 4

    function childSel(i) {
        return { controller: parentSel.controller, position: parentSel.position, path: (parentSel.path || []).concat([[routine, i]]) }
    }

    function reorder(from, to) {
        if (to < 0 || to >= children.length) return
        const copy = JSON.parse(JSON.stringify(parentAction))
        const list = copy.Actions[routine].Actions
        const item = list.splice(from, 1)[0]
        list.splice(to, 0, item)
        Store.updateAction(parentSel, copy)
    }

    function accepts(uuid) {
        const d = Store.definition(uuid)
        if (!d) return qsTr("Unknown action")
        if (box.isMulti && d.supportedInMultiActions === false) return qsTr("%1 cannot be part of a multi action.").arg(d.name)
        if (!box.isMulti && d.supportedInKeyLogic === false && box.controllerFilter === "Keypad") return qsTr("%1 cannot be used here.").arg(d.name)
        if (d.controllers.indexOf(box.controllerFilter) < 0 && uuid !== "com.elgato.streamdeck.multiactions.delay") return qsTr("%1 does not work on a %2.").arg(d.name).arg(box.controllerFilter === "Encoder" ? qsTr("dial") : qsTr("key"))
        return ""
    }

    FieldLabel { text: box.title }

    Rectangle {
        Layout.fillWidth: true
        Layout.leftMargin: 14
        Layout.rightMargin: 14
        Layout.preferredHeight: Math.max(52, list.contentHeight + 12)
        radius: Ui.small
        color: Ui.well
        border.color: drop.containsDrag ? Ui.accent : Ui.border
        border.width: drop.containsDrag ? 2 : 1

        ListView {
            id: list
            anchors.fill: parent
            anchors.margins: 6
            interactive: false
            spacing: 2
            model: box.children
            delegate: ItemDelegate {
                id: row
                required property var modelData
                required property int index
                readonly property var d: Store.definition(modelData.UUID)
                width: list.width
                height: 36
                highlighted: Store.sameSel(Store.selected, box.childSel(index))
                onClicked: Store.selected = box.childSel(index)
                contentItem: RowLayout {
                    spacing: 8
                    Label {
                        text: (row.index + 1) + "."
                        color: Ui.faint
                    }
                    Image {
                        Layout.preferredWidth: 20
                        Layout.preferredHeight: 20
                        source: row.d && row.d.icon ? util.fileUrl(row.d.icon) : ""
                        sourceSize: Qt.size(40, 40)
                    }
                    Label {
                        Layout.fillWidth: true
                        textFormat: Text.PlainText
                        elide: Text.ElideRight
                        color: row.d ? Ui.text : Ui.danger
                        text: {
                            const name = row.d ? row.d.name : row.modelData.Name
                            const t = row.modelData.States && row.modelData.States[0] && row.modelData.States[0].Title
                            if (row.modelData.UUID === "com.elgato.streamdeck.multiactions.delay")
                                return qsTr("Wait %1 ms").arg((row.modelData.Settings && row.modelData.Settings.delay) || 500)
                            return t ? name + " — " + t : name
                        }
                    }
                    ToolButton { text: "↑"; enabled: row.index > 0; onClicked: box.reorder(row.index, row.index - 1) }
                    ToolButton { text: "↓"; enabled: row.index < box.children.length - 1; onClicked: box.reorder(row.index, row.index + 1) }
                    ToolButton { text: "×"; onClicked: Store.removeAction(box.childSel(row.index)) }
                }
            }
        }

        Label {
            anchors.centerIn: parent
            visible: box.children.length === 0
            text: qsTr("Drop actions here")
            color: Ui.faint
        }

        DropArea {
            id: drop
            anchors.fill: parent
            keys: ["action"]
            onDropped: function (ev) {
                const p = ev.source && ev.source.payload
                if (!p || p.kind !== "action") return
                const why = box.accepts(p.uuid)
                if (why) { Store.toast(why, true); return }
                Store.addChild(box.parentSel, box.routine, p.uuid)
                ev.accept()
            }
        }
    }

    RowLayout {
        Layout.leftMargin: 14
        visible: box.isMulti
        Button {
            flat: true
            text: qsTr("+ Delay")
            onClicked: Store.addChild(box.parentSel, box.routine, "com.elgato.streamdeck.multiactions.delay")
        }
    }
}
