import QtQuick
import QtQuick.Controls
import QtQuick.Controls.Material
import QtQuick.Layouts

// Title and image of one state, stored the way the original app stores them
// (States[n].Title, FontFamily, FontSize, FontStyle, TitleAlignment, …).
// Kept compact so it fits beside the settings in a short inspector.
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
    readonly property string alignment: st.TitleAlignment || manifestState.TitleAlignment || "bottom"
    readonly property var imageFilters: [{ name: qsTr("Images"), patterns: ["*.png", "*.jpg", "*.jpeg", "*.gif", "*.svg", "*.webp"] }]

    function save(patch) {
        Store.updateState(sel, stateIndex, patch)
    }

    function pickFile() {
        filePicker.open(qsTr("Choose an image"), imageFilters, false, false, function (paths) {
            if (paths.length) Store.setImage(sel, stateIndex, paths[0])
        })
    }

    contentWidth: availableWidth
    clip: true
    ScrollBar.horizontal.policy: ScrollBar.AlwaysOff

    ColumnLayout {
        width: editor.availableWidth
        spacing: 8

        Item { Layout.preferredHeight: 4 }

        // Image and title side by side.
        RowLayout {
            Layout.fillWidth: true
            Layout.leftMargin: 12
            Layout.rightMargin: 12
            spacing: 10

            Rectangle {
                Layout.preferredWidth: 76
                Layout.preferredHeight: 76
                Layout.alignment: Qt.AlignTop
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
                    ToolTip.visible: containsMouse
                    ToolTip.delay: 500
                    ToolTip.text: qsTr("Click to choose an image or an icon pack. You can also drop an image file here or on a key.")
                }
                DropArea {
                    anchors.fill: parent
                    keys: ["text/uri-list"]
                    onDropped: function (ev) {
                        if (ev.hasUrls) Store.setImage(editor.sel, editor.stateIndex, util.localPath(ev.urls[0]))
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
                Layout.preferredWidth: 1
                spacing: 4
                visible: editor.titleAllowed

                Label {
                    text: qsTr("Title")
                    color: Ui.dim
                    font.pixelSize: 12
                }
                Rectangle {
                    Layout.fillWidth: true
                    Layout.preferredHeight: 56
                    radius: Ui.small
                    color: Ui.well
                    border.color: titleField.activeFocus ? Ui.accent : Ui.border
                    Flickable {
                        id: titleFlick
                        anchors.fill: parent
                        anchors.margins: 6
                        clip: true
                        contentHeight: titleField.implicitHeight
                        TextEdit {
                            id: titleField
                            width: titleFlick.width
                            text: editor.st.Title || ""
                            color: Ui.text
                            selectionColor: Ui.accent
                            selectedTextColor: Ui.accentText
                            font.pixelSize: 13
                            wrapMode: TextEdit.NoWrap
                            // Saved when focus leaves, so typing does not redraw every keystroke.
                            onActiveFocusChanged: if (!activeFocus && text !== (editor.st.Title || "")) editor.save({ Title: text })
                            Keys.onPressed: function (e) {
                                if (e.key === Qt.Key_Return && (e.modifiers & Qt.ControlModifier)) { editor.save({ Title: text }); e.accepted = true }
                            }
                        }
                        Label {
                            visible: titleField.text.length === 0 && !titleField.activeFocus
                            text: qsTr("No title")
                            color: Ui.faint
                            font.pixelSize: 13
                        }
                    }
                }
            }
        }

        // Title options in tight rows: no Material control is taller than it must be.
        GridLayout {
            Layout.fillWidth: true
            Layout.leftMargin: 12
            Layout.rightMargin: 12
            visible: editor.titleAllowed
            columns: 2
            columnSpacing: 8
            rowSpacing: 6

            Label { text: qsTr("Show"); color: Ui.dim; font.pixelSize: 12 }
            RowLayout {
                Layout.fillWidth: true
                spacing: 4
                Switch {
                    padding: 0
                    checked: editor.st.ShowTitle !== false
                    onToggled: editor.save({ ShowTitle: checked })
                }
                Item { Layout.fillWidth: true }
                Repeater {
                    model: [{ v: "top", t: "⤒", tip: qsTr("Top") }, { v: "middle", t: "≡", tip: qsTr("Middle") }, { v: "bottom", t: "⤓", tip: qsTr("Bottom") }]
                    delegate: SmallToggle {
                        required property var modelData
                        text: modelData.t
                        checked: editor.alignment === modelData.v
                        tip: modelData.tip
                        onClicked: editor.save({ TitleAlignment: modelData.v })
                    }
                }
                SmallToggle {
                    text: "U"
                    font.underline: true
                    checked: !!editor.st.FontUnderline
                    tip: qsTr("Underline")
                    onClicked: editor.save({ FontUnderline: !checked })
                }
            }

            Label { text: qsTr("Font"); color: Ui.dim; font.pixelSize: 12 }
            RowLayout {
                Layout.fillWidth: true
                spacing: 4
                CompactCombo {
                    Layout.fillWidth: true
                    Layout.preferredWidth: 1
                    model: [qsTr("Default")].concat(Store.fonts)
                    currentIndex: Math.max(0, Store.fonts.indexOf(editor.st.FontFamily || "") + 1)
                    onActivated: function (i) { editor.save({ FontFamily: i === 0 ? "" : Store.fonts[i - 1] }) }
                }
                CompactCombo {
                    Layout.preferredWidth: 74
                    readonly property var sizes: [7, 8, 9, 10, 11, 12, 13, 14, 16, 18, 20, 24, 28, 32, 40]
                    readonly property int current: editor.st.FontSize || editor.manifestState.FontSize || 12
                    model: sizes
                    currentIndex: Math.max(0, sizes.indexOf(current))
                    displayText: String(current)
                    onActivated: function (i) { editor.save({ FontSize: sizes[i] }) }
                }
            }

            Label { text: qsTr("Style"); color: Ui.dim; font.pixelSize: 12 }
            CompactCombo {
                Layout.fillWidth: true
                readonly property var values: ["Regular", "Bold", "Italic", "Bold Italic"]
                model: [qsTr("Regular"), qsTr("Bold"), qsTr("Italic"), qsTr("Bold Italic")]
                currentIndex: Math.max(0, values.indexOf(editor.st.FontStyle || "Regular"))
                onActivated: function (i) { editor.save({ FontStyle: values[i] === "Regular" ? "" : values[i] }) }
            }

            Label { text: qsTr("Color"); color: Ui.dim; font.pixelSize: 12 }
            ColorField {
                Layout.fillWidth: true
                value: editor.st.TitleColor || editor.manifestState.TitleColor || "#ffffff"
                onPicked: function (c) { editor.save({ TitleColor: c }) }
            }
        }

        Item { Layout.preferredHeight: 8 }
    }

    // A small square toggle for alignment and underline.
    component SmallToggle: AbstractButton {
        property string tip: ""
        implicitWidth: 26
        implicitHeight: 26
        hoverEnabled: true
        background: Rectangle {
            radius: Ui.small
            color: parent.checked ? Ui.accent : (parent.hovered ? Ui.raised : "transparent")
            border.color: parent.checked ? Ui.accent : Ui.border
        }
        contentItem: Text {
            text: parent.text
            font: parent.font
            color: parent.checked ? Ui.accentText : Ui.text
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
        }
        ToolTip.visible: hovered && tip.length > 0
        ToolTip.delay: 500
        ToolTip.text: tip
    }

    // Material's ComboBox is 48 px tall with a wide indicator; this one is not.
    component CompactCombo: ComboBox {
        implicitHeight: 30
        font.pixelSize: 12
        topInset: 0
        bottomInset: 0
        Material.roundedScale: Material.SmallScale
    }

    IconLibrary {
        id: library
        onPicked: function (path) { Store.setImage(editor.sel, editor.stateIndex, path) }
    }
}
