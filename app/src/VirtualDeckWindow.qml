import QtQuick
import QtQuick.Controls
import QtQuick.Controls.Material
import QtQuick.Layouts
import "qml"

// The Virtual Stream Deck: a deck that lives on screen. Keys respond to
// press and release separately, so hold actions work as on hardware.
ApplicationWindow {
    id: win
    visible: true
    color: Ui.window
    title: deck ? deck.name : qsTr("Virtual Stream Deck")
    width: deck ? Math.max(320, deck.columns * (keySize + gap) + gap + 24) : 640
    height: deck ? deck.rows * (keySize + gap) + gap + 74 : 400

    Material.theme: Material.Dark
    Material.accent: theme.accent

    property int keySize: 76
    readonly property int gap: 8
    readonly property var virtuals: Store.devices.filter(d => d.virtual)
    property string deckId: ""
    readonly property var deck: virtuals.find(d => d.id === deckId) || null

    onVirtualsChanged: {
        if (!deck && virtuals.length) {
            const wanted = virtuals.find(d => d.id === startDevice)
            deckId = (wanted || virtuals[0]).id
        }
    }

    Shortcut { sequence: "Q"; onActivated: Qt.quit() }
    Shortcut { sequence: "Escape"; onActivated: Qt.quit() }
    Shortcut { sequence: "PgDown"; onActivated: Store.call("pageStep", { device: win.deckId, delta: 1 }) }
    Shortcut { sequence: "PgUp"; onActivated: Store.call("pageStep", { device: win.deckId, delta: -1 }) }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 12
        spacing: 10

        RowLayout {
            Layout.fillWidth: true
            ComboBox {
                visible: win.virtuals.length > 1
                model: win.virtuals.map(d => d.name)
                currentIndex: win.virtuals.findIndex(d => d.id === win.deckId)
                onActivated: function (i) { win.deckId = win.virtuals[i].id }
            }
            Label {
                visible: win.virtuals.length <= 1
                text: win.deck ? win.deck.profileName + (win.deck.pageCount > 1 ? "  ·  " + qsTr("page %1 of %2").arg(win.deck.pageIndex + 1).arg(win.deck.pageCount) : "") : ""
                color: Ui.dim
                textFormat: Text.PlainText
                elide: Text.ElideRight
                Layout.fillWidth: true
            }
            ToolButton {
                text: "‹"
                visible: !!win.deck && (win.deck.pageCount > 1 || win.deck.inFolder)
                onClicked: win.deck.inFolder ? Store.call("closeFolder", { device: win.deckId }) : Store.call("pageStep", { device: win.deckId, delta: -1 })
            }
            ToolButton {
                text: "›"
                visible: !!win.deck && win.deck.pageCount > 1
                onClicked: Store.call("pageStep", { device: win.deckId, delta: 1 })
            }
            ToolButton {
                text: "📌"
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Float and keep on every workspace")
                // Wayland clients cannot raise themselves; Hyprland can pin a floating window.
                onClicked: {
                    util.launch("hyprctl", ["dispatch", "setfloating", "class:^(streamdeck-vsd)$"])
                    util.launch("hyprctl", ["dispatch", "pin", "class:^(streamdeck-vsd)$"])
                }
            }
            ToolButton {
                text: "✎"
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Edit in Stream Deck")
                onClicked: util.launch("streamdeck", [])
            }
        }

        Grid {
            Layout.alignment: Qt.AlignHCenter
            columns: win.deck ? win.deck.columns : 1
            spacing: win.gap
            Repeater {
                model: win.deck ? win.deck.columns * win.deck.rows : 0
                delegate: Rectangle {
                    id: key
                    required property int index
                    property int revision: daemon.frameRevision(win.deckId + "/key-" + index)
                    readonly property string position: (index % win.deck.columns) + "," + Math.floor(index / win.deck.columns)
                    width: win.keySize
                    height: win.keySize
                    radius: Ui.small + 6
                    color: "#000"
                    border.color: area.containsMouse ? "#4a4a52" : "#26262b"
                    scale: area.pressed ? 0.95 : 1

                    Behavior on scale { NumberAnimation { duration: 60 } }

                    Image {
                        anchors.fill: parent
                        anchors.margins: 3
                        source: "image://frames/" + win.deckId + "/key-" + key.index + "?" + key.revision
                        cache: false
                        smooth: true
                        mipmap: true
                    }
                    Connections {
                        target: Store
                        function onFrameChanged(device, target, index, rev) {
                            if (device === win.deckId && target === "key" && index === key.index) key.revision = rev
                        }
                    }
                    MouseArea {
                        id: area
                        anchors.fill: parent
                        hoverEnabled: true
                        onPressed: Store.call("press", { device: win.deckId, position: key.position, down: true })
                        onReleased: Store.call("press", { device: win.deckId, position: key.position, down: false })
                        onCanceled: Store.call("press", { device: win.deckId, position: key.position, down: false })
                    }
                }
            }
        }

        Label {
            Layout.alignment: Qt.AlignHCenter
            visible: !Store.connected || win.virtuals.length === 0
            color: Ui.dim
            text: !Store.connected ? qsTr("The Stream Deck service is not running.") : qsTr("No Virtual Stream Deck. Add one in Preferences › Devices.")
        }
    }
}
