import QtQuick
import QtQuick.Controls
import QtQuick.Controls.Material
import QtQuick.Layouts

Rectangle {
    id: help
    signal closeRequested()
    color: "#c0000000"

    MouseArea {
        anchors.fill: parent
        onClicked: help.closeRequested()
    }

    Rectangle {
        anchors.centerIn: parent
        width: 480
        height: shortcuts.implicitHeight + 76
        radius: Ui.radius
        color: Ui.panel
        border.color: Ui.border

        Label {
            id: heading
            x: 24
            y: 18
            text: qsTr("Keyboard shortcuts")
            font.pixelSize: 17
            font.bold: true
        }

        Column {
            id: shortcuts
            anchors.top: heading.bottom
            anchors.topMargin: 14
            anchors.left: parent.left
            anchors.leftMargin: 24
            spacing: 7
            Repeater {
                model: [
                    ["Space", qsTr("Press the selected key")],
                    ["Delete", qsTr("Clear the selected key")],
                    ["Ctrl + C / X / V", qsTr("Copy, cut, paste a key")],
                    ["Ctrl + Z", qsTr("Undo")],
                    ["Ctrl + Shift + Z", qsTr("Redo")],
                    ["PgUp / PgDn", qsTr("Previous / next page")],
                    ["Esc", qsTr("Deselect, or leave a folder")],
                    ["Ctrl + F", qsTr("Search actions")],
                    ["Ctrl + ,", qsTr("Preferences")],
                    [qsTr("Drag + Ctrl"), qsTr("Copy a key instead of moving it")],
                    ["?", qsTr("Show this help")],
                    ["Q", qsTr("Quit")]
                ]
                delegate: Row {
                    required property var modelData
                    spacing: 16
                    Label {
                        width: 150
                        text: modelData[0]
                        color: Ui.accent
                        font.family: "monospace"
                    }
                    Label {
                        text: modelData[1]
                        color: Ui.text
                    }
                }
            }
        }
    }
}
