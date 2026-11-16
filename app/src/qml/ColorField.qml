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

    readonly property var swatches: ["#ffffff", "#000000", "#ff453a", "#ff9f0a", "#ffd60a", "#32d74b", "#64d2ff", "#0a84ff", "#bf5af2", "#ff375f"]

    Repeater {
        model: field.swatches
        delegate: Rectangle {
            required property string modelData
            width: 18
            height: 18
            radius: 9
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
        Layout.preferredWidth: 86
        text: field.value
        validator: RegularExpressionValidator { regularExpression: /#[0-9a-fA-F]{6}/ }
        onEditingFinished: if (acceptableInput && text !== field.value) field.picked(text.toLowerCase())
    }
}
