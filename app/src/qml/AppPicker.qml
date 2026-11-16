import QtQuick
import QtQuick.Controls
import QtQuick.Controls.Material
import QtQuick.Layouts

// Installed applications from their desktop entries, searchable.
Dialog {
    id: picker
    signal picked(var app)

    title: qsTr("Choose an application")
    modal: true
    anchors.centerIn: Overlay.overlay
    width: 460
    height: 560
    standardButtons: Dialog.Cancel

    property string query: ""
    readonly property var filtered: {
        const q = query.trim().toLowerCase()
        return Store.applications.filter(a => !q || a.name.toLowerCase().indexOf(q) >= 0 || a.id.toLowerCase().indexOf(q) >= 0)
    }

    onOpened: {
        Store.loadApplications()
        search.text = ""
        search.forceActiveFocus()
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 8

        TextField {
            id: search
            Layout.fillWidth: true
            placeholderText: qsTr("Search")
            onTextChanged: picker.query = text
            Keys.onDownPressed: list.forceActiveFocus()
            onAccepted: if (picker.filtered.length) { picker.picked(picker.filtered[0]); picker.close() }
        }

        ListView {
            id: list
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: picker.filtered
            keyNavigationEnabled: true
            ScrollBar.vertical: ScrollBar {}
            delegate: ItemDelegate {
                required property var modelData
                width: list.width
                height: 44
                highlighted: ListView.isCurrentItem
                onClicked: { picker.picked(modelData); picker.close() }
                Keys.onReturnPressed: { picker.picked(modelData); picker.close() }
                contentItem: RowLayout {
                    spacing: 12
                    Image {
                        Layout.preferredWidth: 28
                        Layout.preferredHeight: 28
                        source: modelData.iconPath ? util.fileUrl(modelData.iconPath) : ""
                        sourceSize: Qt.size(56, 56)
                        fillMode: Image.PreserveAspectFit
                    }
                    Label {
                        Layout.fillWidth: true
                        text: modelData.name
                        textFormat: Text.PlainText
                        elide: Text.ElideRight
                    }
                }
            }
        }
    }
}
