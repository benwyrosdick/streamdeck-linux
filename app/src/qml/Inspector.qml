import QtQuick
import QtQuick.Controls
import QtQuick.Controls.Material
import QtQuick.Layouts

Rectangle {
    id: inspector
    color: Ui.panel

    readonly property var sel: Store.selected
    readonly property var action: { Store.revision; return Store.actionAt(sel) }
    readonly property var def: action ? Store.definition(action.UUID) : null
    readonly property int stateCount: action && action.States ? Math.max(1, action.States.length) : 1
    property int stateIndex: 0

    onActionChanged: if (!action || stateIndex >= stateCount) stateIndex = action ? (action.State || 0) : 0
    onSelChanged: stateIndex = action ? (action.State || 0) : 0

    // Breadcrumb from the key down to the child being edited.
    readonly property var crumbs: {
        Store.revision
        if (!sel) return []
        const out = []
        for (let i = 0; i <= (sel.path || []).length; i++) {
            const s = { controller: sel.controller, position: sel.position, path: (sel.path || []).slice(0, i) }
            const a = Store.actionAt(s)
            const d = a ? Store.definition(a.UUID) : null
            out.push({ sel: s, name: d ? d.name : (a ? a.Name : "") })
        }
        return out
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0
        visible: !!inspector.action

        // Header
        RowLayout {
            Layout.fillWidth: true
            Layout.leftMargin: 14
            Layout.rightMargin: 10
            Layout.topMargin: 8
            Layout.bottomMargin: 6
            spacing: 8

            Image {
                Layout.preferredWidth: 22
                Layout.preferredHeight: 22
                source: inspector.def && inspector.def.icon ? util.fileUrl(inspector.def.icon) : ""
                sourceSize: Qt.size(44, 44)
            }

            Repeater {
                model: inspector.crumbs
                delegate: RowLayout {
                    required property var modelData
                    required property int index
                    spacing: 6
                    Label {
                        visible: index > 0
                        text: "›"
                        color: Ui.faint
                    }
                    Label {
                        text: modelData.name
                        textFormat: Text.PlainText
                        font.bold: index === inspector.crumbs.length - 1
                        font.pixelSize: 15
                        color: index === inspector.crumbs.length - 1 ? Ui.text : Ui.accent
                        MouseArea {
                            anchors.fill: parent
                            enabled: index < inspector.crumbs.length - 1
                            cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                            onClicked: Store.selected = modelData.sel
                        }
                    }
                }
            }

            Label {
                text: inspector.def ? "· " + inspector.def.pluginName : qsTr("· plugin not installed")
                textFormat: Text.PlainText
                color: inspector.def ? Ui.faint : Ui.danger
            }

            Item { Layout.fillWidth: true }

            TabBar {
                visible: inspector.stateCount > 1
                Repeater {
                    model: inspector.stateCount
                    delegate: TabButton {
                        required property int index
                        text: qsTr("State %1").arg(index + 1)
                        width: implicitWidth
                        checked: inspector.stateIndex === index
                        onClicked: inspector.stateIndex = index
                    }
                }
            }

            ToolButton {
                text: "▶"
                enabled: !inspector.sel || !(inspector.sel.path && inspector.sel.path.length)
                onClicked: Store.press(inspector.sel)
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Try it (Space)")
            }
            ToolButton {
                text: "🗑"
                onClicked: Store.removeAction(inspector.sel)
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Remove (Delete)")
            }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 1
            color: Ui.border
        }

        RowLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 0

            AppearanceEditor {
                Layout.preferredWidth: 340
                Layout.fillHeight: true
                sel: inspector.sel
                action: inspector.action
                def: inspector.def
                stateIndex: inspector.stateIndex
            }

            Rectangle {
                Layout.preferredWidth: 1
                Layout.fillHeight: true
                color: Ui.border
            }

            Loader {
                Layout.fillWidth: true
                Layout.fillHeight: true
                active: !!inspector.action
                sourceComponent: !inspector.def ? missingPlugin : (inspector.def.builtin ? builtinView : pluginView)
            }
        }
    }

    Component {
        id: builtinView
        BuiltinInspector {
            sel: inspector.sel
            action: inspector.action
            def: inspector.def
            stateIndex: inspector.stateIndex
        }
    }

    Component {
        id: pluginView
        PluginInspector {
            context: inspector.action ? inspector.action.ActionID : ""
        }
    }

    Component {
        id: missingPlugin
        Item {
            Label {
                anchors.centerIn: parent
                width: parent.width - 40
                wrapMode: Text.WordWrap
                horizontalAlignment: Text.AlignHCenter
                textFormat: Text.PlainText
                color: Ui.dim
                text: inspector.action ? qsTr("This key uses “%1” from a plugin that is not installed (%2). Install the plugin to use it, or clear the key.").arg(inspector.action.Name).arg(inspector.action.Plugin ? inspector.action.Plugin.UUID : inspector.action.UUID) : ""
            }
        }
    }

    Label {
        anchors.centerIn: parent
        visible: !inspector.action
        width: parent.width - 60
        wrapMode: Text.WordWrap
        horizontalAlignment: Text.AlignHCenter
        color: Ui.dim
        text: inspector.sel ? qsTr("Drag an action from the list onto this key, or double-click one to place it.") : qsTr("Select a key to edit it. Press ? for keyboard shortcuts.")
    }
}
