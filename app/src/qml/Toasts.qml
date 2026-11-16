import QtQuick
import QtQuick.Controls
import QtQuick.Controls.Material

// Brief messages along the bottom edge: errors stay longer than notes.
Column {
    id: toasts
    spacing: 8
    width: 520

    ListModel { id: messages }

    Connections {
        target: Store
        function onToast(text, error) {
            messages.append({ text: text, error: error, born: Date.now() })
            if (messages.count > 4) messages.remove(0)
        }
    }

    Timer {
        interval: 500
        running: messages.count > 0
        repeat: true
        onTriggered: {
            const now = Date.now()
            for (let i = messages.count - 1; i >= 0; i--) {
                const m = messages.get(i)
                if (now - m.born > (m.error ? 6000 : 3000)) messages.remove(i)
            }
        }
    }

    Repeater {
        model: messages
        delegate: Rectangle {
            required property string text
            required property bool error
            required property int index
            anchors.horizontalCenter: toasts.horizontalCenter
            width: Math.min(toasts.width, label.implicitWidth + 40)
            height: label.implicitHeight + 20
            radius: Ui.small + 2
            color: error ? "#3a1714" : Ui.raised
            border.color: error ? Ui.danger : Ui.border
            Label {
                id: label
                anchors.centerIn: parent
                width: Math.min(implicitWidth, toasts.width - 40)
                text: parent.text
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
                color: Ui.text
            }
            MouseArea {
                anchors.fill: parent
                onClicked: messages.remove(parent.index)
            }
        }
    }
}
