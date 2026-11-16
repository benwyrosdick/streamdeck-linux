import QtQuick
import QtQuick.Controls
import QtQuick.Controls.Material
import QtQuick.Layouts

// Installed icon packs (.streamDeckIconPack) and the built-in key art.
Dialog {
    id: lib
    signal picked(string path)

    title: qsTr("Icon library")
    modal: true
    anchors.centerIn: Overlay.overlay
    width: Math.min(820, Overlay.overlay ? Overlay.overlay.width - 80 : 820)
    height: Math.min(640, Overlay.overlay ? Overlay.overlay.height - 80 : 640)
    standardButtons: Dialog.Close

    property var packs: []
    property int packIndex: 0
    property string query: ""

    readonly property var icons: {
        const pack = packs[packIndex]
        if (!pack) return []
        const q = query.trim().toLowerCase()
        return pack.icons.filter(i => !q || String(i.name || "").toLowerCase().indexOf(q) >= 0 || (i.tags || []).join(" ").toLowerCase().indexOf(q) >= 0)
    }

    onOpened: refresh()

    function refresh() {
        Store.call("listIconPacks", {}, function (r) {
            // Built-in key art first, so there is always something to pick.
            const builtin = { id: "builtin", name: qsTr("Built-in"), icons: [] }
            for (const p of Store.plugins) {
                if (!p.builtin) continue
                for (const a of p.actions)
                    for (const s of a.states)
                        if (s.Image) builtin.icons.push({ path: p.dir + "/" + s.Image + ".svg", name: a.name, tags: [] })
            }
            lib.packs = [builtin].concat(r || [])
            if (lib.packIndex >= lib.packs.length) lib.packIndex = 0
        })
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 8

        RowLayout {
            Layout.fillWidth: true
            ComboBox {
                Layout.preferredWidth: 260
                model: lib.packs.map(p => p.name || p.id)
                currentIndex: lib.packIndex
                onActivated: function (i) { lib.packIndex = i }
            }
            TextField {
                Layout.fillWidth: true
                placeholderText: qsTr("Search icons")
                onTextChanged: lib.query = text
            }
            Button {
                text: qsTr("Install pack…")
                onClicked: filePicker.open(qsTr("Install an icon pack"), [{ name: qsTr("Icon packs"), patterns: ["*.streamDeckIconPack"] }], false, false, function (p) {
                    if (p.length) Store.call("installIconPack", { path: p[0] }, function () { lib.refresh() })
                })
            }
        }

        GridView {
            id: grid
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            cellWidth: 92
            cellHeight: 108
            model: lib.icons
            ScrollBar.vertical: ScrollBar {}
            delegate: ItemDelegate {
                required property var modelData
                width: grid.cellWidth - 6
                height: grid.cellHeight - 6
                onClicked: { lib.picked(modelData.path); lib.close() }
                contentItem: ColumnLayout {
                    spacing: 4
                    Image {
                        Layout.alignment: Qt.AlignHCenter
                        Layout.preferredWidth: 64
                        Layout.preferredHeight: 64
                        source: util.fileUrl(modelData.path)
                        sourceSize: Qt.size(128, 128)
                        asynchronous: true
                        fillMode: Image.PreserveAspectFit
                    }
                    Label {
                        Layout.fillWidth: true
                        horizontalAlignment: Text.AlignHCenter
                        text: modelData.name || ""
                        textFormat: Text.PlainText
                        elide: Text.ElideRight
                        font.pixelSize: 11
                        color: Ui.dim
                    }
                }
            }
        }

        Label {
            visible: lib.icons.length === 0
            Layout.alignment: Qt.AlignHCenter
            color: Ui.dim
            text: qsTr("No icons here. Icon packs (.streamDeckIconPack) can be installed above or dropped on the window.")
        }
    }
}
