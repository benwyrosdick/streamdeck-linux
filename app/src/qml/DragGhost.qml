import QtQuick
import QtQuick.Controls

// What follows the pointer while dragging an action or a key. It lifts
// itself into the window overlay so it draws above every panel.
Item {
    id: ghost
    property Item handle             // the MouseArea driving the drag
    property var payload: ({})       // {kind: "action", uuid} or {kind: "key", sel}
    property url icon
    property bool copy: false
    property string kindKey: "action"

    width: 56
    height: 56
    visible: Drag.active
    Drag.active: handle ? handle.drag.active : false
    Drag.keys: [kindKey]
    Drag.hotSpot.x: width / 2
    Drag.hotSpot.y: height / 2

    states: State {
        when: ghost.Drag.active
        ParentChange {
            target: ghost
            parent: Overlay.overlay
        }
    }

    Rectangle {
        anchors.fill: parent
        radius: Ui.small
        color: Ui.raised
        border.color: Ui.accent
        border.width: 2
        opacity: 0.92
        Image {
            anchors.fill: parent
            anchors.margins: 6
            source: ghost.icon
            sourceSize: Qt.size(96, 96)
            fillMode: Image.PreserveAspectFit
        }
    }

    // Centres the ghost on the press point. Drops go to whatever is under
    // the hot spot, so it has to start under the pointer and stay there.
    function grab(mouse) {
        const p = handle.mapToItem(parent, mouse.x, mouse.y)
        x = p.x - width / 2
        y = p.y - height / 2
    }

    // Returns the ghost to its origin after a drop.
    function finish(mouse) {
        copy = !!(mouse && (mouse.modifiers & Qt.ControlModifier))
        if (Drag.active)
            Drag.drop()
        x = 0
        y = 0
    }
}
