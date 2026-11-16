import QtQuick
import QtQuick.Controls
import QtQuick.Controls.Material
import QtQuick.Layouts

// A swatch row plus a hex field; enough for title colours.
RowLayout {
    id: field
    property string value: "#ffffff"
    signal picked(string color)
    spacing: 4

    readonly property var swatches: ["#ffffff", "#000000", "#ff453a", "#ffd60a", "#32d74b", "#0a84ff", "#bf5af2"]

    Repeater {
        model: field.swatches
        delegate: Rectangle {
            required property string modelData
            width: 16
            height: 16
            radius: 8
            color: modelData
            border.width: field.value.toLowerCase() === modelData ? 2 : 1
            border.color: field.value.toLowerCase() === modelData ? Ui.accent : Ui.border
            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onClicked: field.picked(parent.modelData)
            }
        }
    }

    TextField {
        Layout.fillWidth: true
        Layout.minimumWidth: 64
        Layout.preferredHeight: 30
        topInset: 0
        bottomInset: 0
        font.pixelSize: 12
        text: field.value
        validator: RegularExpressionValidator { regularExpression: /#[0-9a-fA-F]{6}/ }
        onEditingFinished: if (acceptableInput && text !== field.value) field.picked(text.toLowerCase())
    }
}
