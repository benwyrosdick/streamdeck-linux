import QtQuick
import QtQuick.Controls
import QtQuick.Controls.Material
import QtQuick.Layouts

Item {
    id: canvas
    readonly property var dev: Store.device
    readonly property int cols: dev ? dev.columns : 5
    readonly property int rows: dev ? dev.rows : 3
    readonly property int encoders: dev ? dev.encoders : 0
    readonly property int touchPoints: dev ? dev.touchPoints : 0
    readonly property var screen: dev ? dev.screen : null
    readonly property real gap: Math.max(8, cell * 0.12)
    readonly property real gridWidth: cols * cell + (cols - 1) * gap

    // Size keys to fill the space while leaving room for a strip or bar.
    readonly property real extraRows: encoders > 0 ? 1.7 : (screen ? 0.8 : 0)
    readonly property real cell: Math.max(40, Math.min(132, (width - 80) / (cols + (cols - 1) * 0.12), (height - 80) / (rows + extraRows + (rows - 1) * 0.12)))

    Rectangle {
        id: body
        anchors.centerIn: parent
        width: canvas.gridWidth + 48
        height: column.implicitHeight + 48
        radius: Math.max(Ui.radius, 18)
        color: "#121215"
        border.color: Ui.border
        visible: !!canvas.dev

        ColumnLayout {
            id: column
            anchors.centerIn: parent
            spacing: canvas.gap * 1.5

            // Neo: info bar above the keys with the two touch points beside it.
            RowLayout {
                visible: !!canvas.screen && canvas.encoders === 0
                Layout.alignment: Qt.AlignHCenter
                spacing: canvas.gap
                TouchPoint { index: 0; glyph: "‹"; visible: canvas.touchPoints > 0 }
                KeyCell {
                    Layout.preferredWidth: canvas.screen ? canvas.cell * 2.4 : 0
                    Layout.preferredHeight: canvas.screen ? canvas.cell * 2.4 * canvas.screen[1] / canvas.screen[0] : 0
                    controller: "Neo"
                    position: "0,0"
                    frameTarget: "screen"
                    frameIndex: 0
                    radius: 6
                }
                TouchPoint { index: 1; glyph: "›"; visible: canvas.touchPoints > 0 }
            }

            Grid {
                id: grid
                Layout.alignment: Qt.AlignHCenter
                columns: canvas.cols
                spacing: canvas.gap
                Repeater {
                    model: canvas.cols * canvas.rows
                    delegate: KeyCell {
                        required property int index
                        width: canvas.cell
                        height: canvas.cell
                        controller: "Keypad"
                        position: (index % canvas.cols) + "," + Math.floor(index / canvas.cols)
                        frameIndex: index
                    }
                }
            }

            // Stream Deck +: touch strip segments over the dials.
            Item {
                visible: canvas.encoders > 0
                Layout.alignment: Qt.AlignHCenter
                Layout.preferredWidth: canvas.gridWidth
                Layout.preferredHeight: canvas.encoders > 0 ? canvas.gridWidth / 8 + canvas.cell * 0.75 + canvas.gap : 0

                Rectangle {
                    id: strip
                    width: parent.width
                    height: parent.width / 8
                    radius: 6
                    color: "#000"
                    border.color: Ui.border
                    clip: true

                    Image {
                        id: stripImage
                        property int revision: 0
                        anchors.fill: parent
                        anchors.margins: 2
                        source: Store.deviceId && canvas.encoders > 0 ? "image://frames/" + Store.deviceId + "/screen-0?" + revision : ""
                        cache: false
                        smooth: true
                        Connections {
                            target: Store
                            function onFrameChanged(device, target, index, rev) {
                                if (device === Store.deviceId && target === "screen") stripImage.revision = rev
                            }
                        }
                    }
                }

                Row {
                    anchors.top: strip.bottom
                    anchors.topMargin: canvas.gap
                    anchors.horizontalCenter: parent.horizontalCenter
                    Repeater {
                        model: canvas.encoders
                        delegate: Item {
                            required property int index
                            width: canvas.gridWidth / canvas.encoders
                            height: canvas.cell * 0.75
                            KeyCell {
                                anchors.centerIn: parent
                                width: parent.height
                                height: parent.height
                                radius: width / 2
                                controller: "Encoder"
                                position: index + ",0"
                                showFrame: false
                                frameTarget: "dial"
                                frameIndex: index
                            }
                            Label {
                                anchors.centerIn: parent
                                text: "◎"
                                color: Ui.faint
                                font.pixelSize: parent.height * 0.5
                            }
                        }
                    }
                }

                // Clicking a strip segment selects its dial.
                Row {
                    anchors.fill: strip
                    Repeater {
                        model: canvas.encoders
                        delegate: MouseArea {
                            required property int index
                            width: strip.width / canvas.encoders
                            height: strip.height
                            onClicked: Store.selected = { controller: "Encoder", position: index + ",0", path: [] }
                            Rectangle {
                                anchors.fill: parent
                                anchors.margins: 2
                                color: "transparent"
                                radius: 4
                                border.width: Store.selected && Store.selected.controller === "Encoder" && Store.selected.position === index + ",0" ? 2 : 0
                                border.color: Ui.accent
                            }
                        }
                    }
                }
            }
        }
    }

    Label {
        anchors.centerIn: parent
        visible: !canvas.dev
        text: qsTr("Connect a Stream Deck, or add a Virtual Stream Deck in Preferences.")
        color: Ui.dim
    }

    component TouchPoint: RoundButton {
        property int index: 0
        property string glyph: ""
        text: glyph
        flat: true
        font.pixelSize: 22
        onClicked: Store.call("pageStep", { device: Store.deviceId, delta: index === 0 ? -1 : 1 })
        ToolTip.visible: hovered
        ToolTip.text: index === 0 ? qsTr("Previous page") : qsTr("Next page")
    }
}
