import QtQuick
import QtQuick.Controls
import QtQuick.Controls.Material
import QtQuick.Layouts

// Title and image of one state, stored the way the original app stores them
// (States[n].Title, FontFamily, FontSize, FontStyle, TitleAlignment, …).
ScrollView {
    id: editor
    property var sel
    property var action
    property var def
    property int stateIndex: 0

    readonly property var st: action && action.States && action.States[stateIndex] ? action.States[stateIndex] : ({})
    readonly property var manifestState: def && def.states && def.states[stateIndex] ? def.states[stateIndex] : ({})
    readonly property bool isDial: sel && sel.controller === "Encoder"
    readonly property bool titleAllowed: !def || def.userTitleEnabled !== false

    function save(patch) {
        Store.updateState(sel, stateIndex, patch)
    }

    contentWidth: availableWidth
    clip: true

    ColumnLayout {
        width: editor.availableWidth
        spacing: 10

        Item { Layout.preferredHeight: 4 }

        RowLayout {
            Layout.fillWidth: true
            Layout.leftMargin: 14
            Layout.rightMargin: 14
            spacing: 12

            // The image well shows the live key; the menu changes the state's image.
            Rectangle {
                Layout.preferredWidth: 84
                Layout.preferredHeight: 84
                radius: Ui.small + 4
                color: "#000"
                border.color: wellMouse.containsMouse ? Ui.accent : Ui.border
                Image {
                    id: wellImage
                    property int revision: 0
                    anchors.fill: parent
                    anchors.margins: 3
                    visible: !editor.isDial && !(editor.sel && editor.sel.path && editor.sel.path.length)
                    cache: false
                    source: visible && Store.device && editor.sel ? "image://frames/" + Store.deviceId + "/key-" + (parseInt(editor.sel.position.split(",")[1]) * Store.device.columns + parseInt(editor.sel.position.split(",")[0])) + "?" + revision : ""
                    Connections {
                        target: Store
                        function onFrameChanged(device, target, index, rev) { wellImage.revision = rev }
                    }
                }
                Image {
                    anchors.fill: parent
                    anchors.margins: 12
                    visible: !wellImage.visible && !!editor.def
                    source: editor.def && editor.def.icon ? util.fileUrl(editor.def.icon) : ""
                    fillMode: Image.PreserveAspectFit
                }
                MouseArea {
                    id: wellMouse
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: imageMenu.popup()
                }
                DropArea {
                    anchors.fill: parent
                    keys: ["text/uri-list"]
                    onDropped: function (ev) {
                        if (ev.hasUrls) editor.chooseImage(util.localPath(ev.urls[0]))
                        ev.accept()
                    }
                }
                Menu {
                    id: imageMenu
                    MenuItem { text: qsTr("Choose file…"); onTriggered: editor.pickFile() }
                    MenuItem { text: qsTr("Icon library…"); onTriggered: library.open() }
                    MenuItem { text: qsTr("Reset to default"); onTriggered: Store.setImage(editor.sel, editor.stateIndex, "") }
                    MenuSeparator { visible: editor.isDial }
                    MenuItem {
                        visible: editor.isDial
                        height: visible ? implicitHeight : 0
                        text: qsTr("Touch strip background…")
                        onTriggered: filePicker.open(qsTr("Choose a background"), editor.imageFilters, false, false, function (paths) {
                            if (paths.length) Store.setImage(editor.sel, 0, paths[0], "background")
                        })
                    }
                }
            }

            ColumnLayout {
                Layout.fillWidth: true
                // A small preferred width lets the layout shrink it so the hint wraps.
                Layout.preferredWidth: 1
                spacing: 2
                Label {
                    text: editor.isDial ? qsTr("Dial icon") : qsTr("Key image")
                    color: Ui.dim
                }
                Label {
                    Layout.fillWidth: true
                    Layout.maximumWidth: editor.availableWidth - 124
                    wrapMode: Text.WordWrap
                    color: Ui.faint
                    font.pixelSize: 11
                    text: qsTr("Click to choose an image or an icon pack. You can also drop a PNG, JPEG, GIF or SVG on a key.")
                }
            }
        }

        FieldLabel { text: qsTr("Title"); visible: editor.titleAllowed }
        ScrollView {
            Layout.fillWidth: true
            Layout.leftMargin: 14
            Layout.rightMargin: 14
            Layout.preferredHeight: 64
            visible: editor.titleAllowed
            TextArea {
                id: titleField
                text: editor.st.Title || ""
                wrapMode: TextEdit.NoWrap
                // Saved when focus leaves, so typing does not re-render every keystroke.
                onActiveFocusChanged: if (!activeFocus && text !== (editor.st.Title || "")) editor.save({ Title: text })
                Keys.onPressed: function (e) {
                    if (e.key === Qt.Key_Return && (e.modifiers & Qt.ControlModifier)) { editor.save({ Title: text }); e.accepted = true }
                }
            }
        }

        GridLayout {
            Layout.fillWidth: true
            Layout.leftMargin: 14
            Layout.rightMargin: 14
            columns: 2
            columnSpacing: 10
            rowSpacing: 6
            visible: editor.titleAllowed

            Label { text: qsTr("Show"); color: Ui.dim }
            Switch {
                checked: editor.st.ShowTitle !== false
                onToggled: editor.save({ ShowTitle: checked })
            }

            Label { text: qsTr("Position"); color: Ui.dim }
            ButtonGroup { id: alignGroup }
            Row {
                spacing: 4
                Repeater {
                    model: [{ v: "top", t: qsTr("Top") }, { v: "middle", t: qsTr("Middle") }, { v: "bottom", t: qsTr("Bottom") }]
                    delegate: Button {
                        required property var modelData
                        text: modelData.t
                        flat: (editor.st.TitleAlignment || editor.manifestState.TitleAlignment || "bottom") !== modelData.v
                        highlighted: !flat
                        implicitWidth: 66
                        onClicked: editor.save({ TitleAlignment: modelData.v })
                    }
                }
            }

            Label { text: qsTr("Font"); color: Ui.dim }
            ComboBox {
                Layout.fillWidth: true
                model: [qsTr("Default")].concat(Store.fonts)
                currentIndex: Math.max(0, Store.fonts.indexOf(editor.st.FontFamily || "") + 1)
                onActivated: function (i) { editor.save({ FontFamily: i === 0 ? "" : Store.fonts[i - 1] }) }
            }

            Label { text: qsTr("Size"); color: Ui.dim }
            RowLayout {
                SpinBox {
                    from: 6
                    to: 40
                    value: editor.st.FontSize || editor.manifestState.FontSize || 12
                    editable: true
                    onValueModified: editor.save({ FontSize: value })
                }
                ComboBox {
                    Layout.fillWidth: true
                    model: [qsTr("Regular"), qsTr("Bold"), qsTr("Italic"), qsTr("Bold Italic")]
                    readonly property var values: ["Regular", "Bold", "Italic", "Bold Italic"]
                    currentIndex: Math.max(0, values.indexOf(editor.st.FontStyle || "Regular"))
                    onActivated: function (i) { editor.save({ FontStyle: values[i] === "Regular" ? "" : values[i] }) }
                }
            }

            Label { text: qsTr("Underline"); color: Ui.dim }
            Switch {
                checked: !!editor.st.FontUnderline
                onToggled: editor.save({ FontUnderline: checked })
            }

            Label { text: qsTr("Color"); color: Ui.dim }
            ColorField {
                value: editor.st.TitleColor || editor.manifestState.TitleColor || "#ffffff"
                onPicked: function (c) { editor.save({ TitleColor: c }) }
            }
        }

        Item { Layout.preferredHeight: 10 }
    }

    readonly property var imageFilters: [{ name: qsTr("Images"), patterns: ["*.png", "*.jpg", "*.jpeg", "*.gif", "*.svg", "*.webp"] }]

    function pickFile() {
        filePicker.open(qsTr("Choose an image"), imageFilters, false, false, function (paths) {
            if (paths.length) editor.chooseImage(paths[0])
        })
    }

    function chooseImage(path) {
        Store.setImage(sel, stateIndex, path)
    }

    IconLibrary {
        id: library
        onPicked: function (path) { editor.chooseImage(path) }
    }
}
