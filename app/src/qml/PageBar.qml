import QtQuick
import QtQuick.Controls
import QtQuick.Controls.Material
import QtQuick.Layouts

Rectangle {
    id: bar
    implicitHeight: 44
    color: Ui.window

    readonly property var pages: Store.profile && Store.profile.manifest ? Store.profile.manifest.Pages.Pages : []
    readonly property bool inFolder: Store.device ? Store.device.inFolder : false

    RowLayout {
        anchors.fill: parent
        anchors.leftMargin: 14
        anchors.rightMargin: 14
        spacing: 6

        Button {
            visible: bar.inFolder
            flat: true
            text: "‹ " + qsTr("Back")
            onClicked: Store.call("closeFolder", { device: Store.deviceId })
            ToolTip.visible: hovered
            ToolTip.text: qsTr("Leave this folder (Esc)")
        }

        Label {
            visible: bar.inFolder
            text: qsTr("Folder")
            color: Ui.dim
        }

        Repeater {
            model: bar.inFolder ? [] : bar.pages
            delegate: Button {
                id: tab
                required property string modelData
                required property int index
                readonly property bool current: modelData.toLowerCase() === Store.pageId.toLowerCase()
                readonly property var pageInfo: Store.page(modelData)
                flat: !current
                highlighted: current
                implicitHeight: 34
                text: pageInfo && pageInfo.Name ? pageInfo.Name : String(index + 1)
                onClicked: Store.selectPage(modelData)
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Page %1 — right-click for options").arg(index + 1)

                MouseArea {
                    anchors.fill: parent
                    acceptedButtons: Qt.RightButton
                    onClicked: pageMenu.popup()
                }

                // A key's action can be dropped on a page tab to move it there.
                DropArea {
                    anchors.fill: parent
                    keys: ["key"]
                    onEntered: if (!tab.current) Store.selectPage(tab.modelData)
                }

                Menu {
                    id: pageMenu
                    MenuItem { text: qsTr("Rename…"); onTriggered: renameDialog.ask(tab.modelData, tab.text) }
                    MenuItem { text: qsTr("Move left"); enabled: tab.index > 0; onTriggered: Store.call("movePage", { profile: Store.profileId, page: tab.modelData, index: tab.index - 1 }) }
                    MenuItem { text: qsTr("Move right"); enabled: tab.index < bar.pages.length - 1; onTriggered: Store.call("movePage", { profile: Store.profileId, page: tab.modelData, index: tab.index + 1 }) }
                    MenuItem { text: qsTr("Start on this page"); onTriggered: Store.call("setDefaultPage", { profile: Store.profileId, page: tab.modelData }) }
                    MenuSeparator {}
                    MenuItem { text: qsTr("Delete page"); enabled: bar.pages.length > 1; onTriggered: Store.call("deletePage", { profile: Store.profileId, page: tab.modelData }) }
                }
            }
        }

        ToolButton {
            visible: !bar.inFolder && !!Store.profileId
            text: "+"
            font.pixelSize: 18
            onClicked: Store.call("addPage", { profile: Store.profileId }, function (id) { if (id) Store.selectPage(id) })
            ToolTip.visible: hovered
            ToolTip.text: qsTr("Add page")
        }

        Item { Layout.fillWidth: true }

        Label {
            visible: !!Store.device
            color: Ui.faint
            text: Store.device && Store.device.pageCount > 1 && !bar.inFolder ? qsTr("PgUp / PgDn switch pages") : ""
            font.pixelSize: 11
        }
    }

    Dialog {
        id: renameDialog
        property string page: ""
        anchors.centerIn: Overlay.overlay
        modal: true
        title: qsTr("Rename page")
        width: 320
        standardButtons: Dialog.Ok | Dialog.Cancel
        function ask(id, current) {
            page = id
            field.text = current
            open()
            field.forceActiveFocus()
            field.selectAll()
        }
        TextField {
            id: field
            implicitWidth: 280
            onAccepted: renameDialog.accept()
        }
        onAccepted: Store.call("renamePage", { profile: Store.profileId, page: page, name: field.text.trim() })
    }
}
