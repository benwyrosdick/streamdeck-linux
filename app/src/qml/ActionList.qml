import QtQuick
import QtQuick.Controls
import QtQuick.Controls.Material
import QtQuick.Layouts

Rectangle {
    id: list
    color: Ui.panel

    property string query: ""
    property var collapsed: ({})

    function focusSearch() {
        search.forceActiveFocus()
        search.selectAll()
    }

    // Categories in a stable order, built-ins first like the original's list.
    readonly property var groups: {
        const q = query.trim().toLowerCase()
        const byCat = {}
        for (const p of Store.plugins) {
            for (const a of p.actions) {
                if (!a.visible) continue
                // Only actions this device has a place for: dials and the Neo bar are optional.
                const dev = Store.device
                const fits = a.controllers.some(c => c === "Keypad" || (c === "Encoder" && dev && dev.encoders > 0) || (c === "Neo" && dev && dev.model === "neo"))
                if (!fits) continue
                const hay = (a.name + " " + a.tooltip + " " + p.name).toLowerCase()
                if (q && hay.indexOf(q) < 0) continue
                const cat = p.category || p.name
                if (!byCat[cat]) byCat[cat] = { name: cat, icon: p.categoryIcon || p.icon, builtin: p.builtin, actions: [] }
                byCat[cat].actions.push(Object.assign({ pluginName: p.name, unsupported: p.unsupported }, a))
            }
        }
        const order = ["Stream Deck", "System", "Multi Action", "Soundboard"]
        return Object.values(byCat).sort(function (a, b) {
            const ia = order.indexOf(a.name), ib = order.indexOf(b.name)
            if (ia >= 0 || ib >= 0) return (ia < 0 ? 99 : ia) - (ib < 0 ? 99 : ib)
            return a.name.localeCompare(b.name)
        })
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        TextField {
            id: search
            Layout.fillWidth: true
            Layout.margins: 10
            placeholderText: qsTr("Search actions")
            onTextChanged: list.query = text
            Keys.onEscapePressed: { text = ""; focus = false }
        }

        ListView {
            id: view
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: list.groups
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar {}

            delegate: Column {
                id: group
                required property var modelData
                width: view.width
                readonly property bool open: list.query.length > 0 || !list.collapsed[modelData.name]

                ItemDelegate {
                    width: parent.width
                    height: 34
                    onClicked: {
                        const c = Object.assign({}, list.collapsed)
                        c[group.modelData.name] = group.open
                        list.collapsed = c
                    }
                    contentItem: RowLayout {
                        spacing: 8
                        Label {
                            text: group.open ? "▾" : "▸"
                            color: Ui.faint
                        }
                        Label {
                            Layout.fillWidth: true
                            text: group.modelData.name
                            textFormat: Text.PlainText
                            font.bold: true
                            color: Ui.dim
                            elide: Text.ElideRight
                        }
                        Label {
                            text: group.modelData.actions.length
                            color: Ui.faint
                            font.pixelSize: 11
                        }
                    }
                }

                Repeater {
                    model: group.open ? group.modelData.actions : []
                    delegate: ItemDelegate {
                        id: row
                        required property var modelData
                        width: group.width
                        height: 40
                        enabled: modelData.osSupported !== false
                        opacity: modelData.unsupported ? 0.5 : 1

                        contentItem: RowLayout {
                            spacing: 10
                            Image {
                                Layout.preferredWidth: 22
                                Layout.preferredHeight: 22
                                Layout.leftMargin: 14
                                source: row.modelData.icon ? util.fileUrl(row.modelData.icon) : ""
                                sourceSize: Qt.size(44, 44)
                                fillMode: Image.PreserveAspectFit
                            }
                            Label {
                                Layout.fillWidth: true
                                text: row.modelData.name
                                textFormat: Text.PlainText
                                elide: Text.ElideRight
                                color: Ui.text
                            }
                            Label {
                                visible: row.modelData.controllers.indexOf("Encoder") >= 0
                                text: "◎"
                                color: Ui.faint
                                ToolTip.visible: hoverTip.containsMouse
                                ToolTip.text: qsTr("Works on dials")
                                MouseArea { id: hoverTip; anchors.fill: parent; hoverEnabled: true }
                            }
                        }

                        MouseArea {
                            id: dragArea
                            anchors.fill: parent
                            drag.target: ghost
                            drag.threshold: 4
                            cursorShape: Qt.OpenHandCursor
                            onPressed: function (mouse) { ghost.grab(mouse) }
                            onReleased: function (mouse) { ghost.finish(mouse) }
                            onDoubleClicked: {
                                if (Store.selected) Store.placeAction(Store.selected, row.modelData.uuid)
                                else Store.toast(qsTr("Select a key first, or drag the action onto one."), false)
                            }
                            hoverEnabled: true
                            ToolTip.visible: containsMouse && !drag.active
                            ToolTip.delay: 600
                            ToolTip.text: (row.modelData.tooltip || row.modelData.name) + (row.modelData.unsupported ? "\n" + row.modelData.unsupported : "")
                        }

                        DragGhost {
                            id: ghost
                            handle: dragArea
                            kindKey: "action"
                            payload: ({ kind: "action", uuid: row.modelData.uuid })
                            icon: row.modelData.icon ? util.fileUrl(row.modelData.icon) : ""
                        }
                    }
                }
            }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 1
            color: Ui.border
        }

        Button {
            Layout.fillWidth: true
            Layout.margins: 8
            flat: true
            text: qsTr("Install plugin…")
            onClicked: filePicker.open(qsTr("Install a Stream Deck plugin"), [{ name: qsTr("Stream Deck plugins"), patterns: ["*.streamDeckPlugin"] }], false, false, function (paths) {
                if (paths.length) Store.installPlugin(paths[0])
            })
        }
    }
}
