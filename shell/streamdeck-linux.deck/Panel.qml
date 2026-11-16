import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Commons as Commons
import qs.Ui
import "Deck.js" as Deck

// Stream Deck in the Omarchy bar: shows the active profile, and opens a
// panel with a live view of the keys, profile and page switching, and
// brightness. It talks to streamdeckd over its control socket.
Panel {
  id: root
  moduleName: "streamdeck-linux.deck"
  ipcTarget: "streamdeck-linux.deck"

  Commons.Translations { id: strings; directory: Qt.resolvedUrl("i18n") }

  readonly property string runtimeDir: (Quickshell.env("STREAMDECK_RUNTIME_DIR") || ((Quickshell.env("XDG_RUNTIME_DIR") || "/tmp") + "/streamdeck-linux"))
  readonly property bool showLabel: setting("showProfile", true) !== false
  // Panel, unlike BarWidget, does not carry the bar's orientation.
  readonly property bool vertical: bar ? bar.vertical === true : false
  readonly property color foreground: bar ? bar.foreground : Commons.Color.foreground
  readonly property color dim: Qt.darker(foreground, 1.55)
  readonly property string fontFamily: bar ? bar.fontFamily : Style.font.family

  property bool serviceUp: false
  property var devices: []
  property string chosenId: ""
  property var profiles: []
  property int revision: 0
  property int nextId: 1
  property var pending: ({})
  readonly property var device: Deck.pick(devices, chosenId)

  // ------------------------------------------------------------------
  // Control socket

  function request(method, params, done) {
    if (!sock.connected) return
    var id = nextId++
    if (done) pending[id] = done
    sock.write(Deck.requestLine(id, method, params))
    sock.flush()
  }

  function onLine(line) {
    var m = Deck.parseLine(line)
    if (m.kind === "reply") {
      var done = pending[m.id]
      delete pending[m.id]
      if (done) done(m.result, m.error)
    } else if (m.kind === "event") {
      if (m.name === "state") devices = Deck.devices(m.data)
      if (m.name === "profilesChanged" || m.name === "state") loadProfiles()
      if (Deck.redrawsKeys(m.name)) revision++
    }
  }

  function loadProfiles() {
    if (!device) { profiles = []; return }
    var id = device.id
    request("listProfiles", { device: id }, function(r) {
      if (root.device && root.device.id === id) root.profiles = Deck.profileOptions(r)
    })
  }

  Socket {
    id: sock
    path: root.runtimeDir + "/control.sock"
    connected: true
    parser: SplitParser { onRead: function(data) { root.onLine(data) } }
    onConnectionStateChanged: {
      root.serviceUp = connected
      root.pending = ({})
      if (connected) {
        root.request("subscribe", {})
        root.request("getState", {}, function(s) { root.devices = Deck.devices(s); root.loadProfiles() })
      } else {
        root.devices = []
      }
    }
  }

  // The service may start after the shell, or restart; keep knocking.
  Timer {
    interval: 3000
    running: !sock.connected
    repeat: true
    onTriggered: { sock.connected = false; sock.connected = true }
  }

  // Plugin-driven key faces change without any event, so re-read the frame
  // files while the panel is open. They are small PNGs on tmpfs.
  Timer {
    interval: 1000
    running: root.opened && root.serviceUp
    repeat: true
    onTriggered: root.revision++
  }

  onDeviceChanged: loadProfiles()

  function press(index) {
    if (!device) return
    request("press", { device: device.id, position: Deck.keyPosition(index, device.columns) })
  }
  function pageStep(delta) { if (device) request("pageStep", { device: device.id, delta: delta }) }
  function closeFolder() { if (device) request("closeFolder", { device: device.id }) }
  function selectProfile(id) { if (device) request("selectProfile", { device: device.id, profile: id }) }
  function setBrightness(v) { if (device) request("setBrightness", { device: device.id, value: Math.round(v) }) }
  function toggleSleep() { if (device) request(device.asleep ? "wake" : "sleep", { device: device.id }) }
  function openEditor() { Quickshell.execDetached(["streamdeck"]); close() }
  function openVirtual() { Quickshell.execDetached(["streamdeck", "vsd"]); close() }
  function startService() { Quickshell.execDetached(["systemctl", "--user", "start", "streamdeckd.service"]) }

  function frameUrl(target, index) {
    return device ? "file://" + Deck.framePath(root.runtimeDir, device.id, target, index) + "?" + root.revision : ""
  }

  readonly property string statusText: {
    if (!serviceUp) return strings.t("status.serviceDown")
    if (!device) return strings.t("status.noDevice")
    if (device.asleep) return strings.t("status.asleep")
    if (device.inFolder) return strings.t("status.inFolder").arg(device.profileName)
    if (device.pageCount > 1) return strings.t("status.page").arg(device.profileName).arg(device.pageIndex + 1).arg(device.pageCount)
    return device.profileName
  }

  implicitWidth: activeButton.implicitWidth
  implicitHeight: activeButton.implicitHeight

  readonly property string barText: showLabel && !vertical && device ? Deck.barLabel(device, 14) : ""
  readonly property string barTooltip: opened ? "" : (device ? device.name + " — " + statusText : statusText)
  // The optical icon button is icon-only; with a label the widget is a text
  // button carrying the glyph, like other labelled bar widgets.
  readonly property Item activeButton: barText !== "" ? labelButton : button

  function onBarPressed(b) {
    if (b === Qt.RightButton || b === Qt.MiddleButton) root.openEditor()
    else root.toggle()
  }

  BarIconButton {
    id: button
    anchors.fill: parent
    visible: root.barText === ""
    bar: root.bar
    text: "󰕰"
    opacity: root.serviceUp && root.device ? 1 : 0.5
    tooltipText: root.barTooltip
    onPressed: function(b) { root.onBarPressed(b) }
    // Scrolling over the icon turns pages, like the deck's own dials.
    onWheelMoved: function(delta) { root.pageStep(delta < 0 ? 1 : -1) }
  }

  WidgetButton {
    id: labelButton
    anchors.fill: parent
    visible: root.barText !== ""
    bar: root.bar
    text: "󰕰  " + root.barText
    fontSize: Style.font.caption
    opacity: root.serviceUp && root.device ? 1 : 0.5
    tooltipText: root.barTooltip
    onPressed: function(b) { root.onBarPressed(b) }
    onWheelMoved: function(delta) { root.pageStep(delta < 0 ? 1 : -1) }
  }

  KeyboardPanel {
    id: panel
    anchorItem: root.activeButton
    owner: root
    bar: root.bar
    open: root.opened
    focusTarget: keyCatcher
    contentWidth: panel.fittedContentWidth(Style.space(Math.max(320, Math.min(560, (root.device ? root.device.columns : 5) * 64 + 40))))
    contentHeight: panel.fittedContentHeight(column.implicitHeight)

    PanelKeyCatcher {
      id: keyCatcher
      anchors.fill: parent
      onMoveRequested: function(dx, dy) { if (dx !== 0) root.pageStep(dx > 0 ? 1 : -1) }
      onReturnRequested: root.closeFolder()
      onActivateRequested: root.openEditor()
      onCloseRequested: root.close()
      onTabRequested: function(direction) { root.switchPanel(direction) }
      onTextKey: function(text) {
        if (text === "e") root.openEditor()
        else if (text === "v") root.openVirtual()
        else if (text === "s") root.toggleSleep()
        else if (text >= "1" && text <= "9" && root.device) root.press(Number(text) - 1)
      }

      Column {
        id: column
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        spacing: Style.space(12)

        // Header: device and what it shows.
        Row {
          width: parent.width
          spacing: Style.space(10)
          Text {
            anchors.verticalCenter: parent.verticalCenter
            text: "󰕰"
            color: root.foreground
            font.family: root.fontFamily
            font.pixelSize: Style.font.display
            textFormat: Text.PlainText
          }
          Column {
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width - Style.space(60)
            spacing: Style.space(2)
            Text {
              width: parent.width
              text: root.device ? root.device.name : strings.t("title")
              color: root.foreground
              font.family: root.fontFamily
              font.pixelSize: Style.font.title
              font.bold: true
              elide: Text.ElideRight
              textFormat: Text.PlainText
            }
            Text {
              width: parent.width
              text: root.statusText
              color: root.dim
              font.family: root.fontFamily
              font.pixelSize: Style.font.caption
              elide: Text.ElideRight
              textFormat: Text.PlainText
            }
          }
        }

        // Several decks: choose which one the panel shows.
        Dropdown {
          visible: root.devices.length > 1
          width: parent.width
          label: strings.t("device")
          value: root.device ? root.device.id : ""
          options: root.devices.map(function(d) { return { value: d.id, label: d.name } })
          fontFamily: root.fontFamily
          onChanged: function(v) { root.chosenId = v }
        }

        // Live keys: click one to press it.
        Grid {
          id: keys
          visible: !!root.device
          anchors.horizontalCenter: parent.horizontalCenter
          columns: root.device ? root.device.columns : 1
          spacing: Style.space(6)
          readonly property real cell: root.device ? Math.min(Style.space(64), (column.width - (columns - 1) * spacing) / columns) : 0
          Repeater {
            model: root.device ? root.device.columns * root.device.rows : 0
            delegate: Rectangle {
              id: key
              required property int index
              width: keys.cell
              height: keys.cell
              radius: Math.max(Style.cornerRadius, Style.space(6))
              color: "black"
              border.width: keyMouse.containsMouse ? Style.space(2) : 1
              border.color: keyMouse.containsMouse ? Commons.Color.accent : Qt.darker(root.foreground, 3)
              scale: keyMouse.pressed ? 0.94 : 1
              Behavior on scale { NumberAnimation { duration: Style.duration(80); easing.type: Easing.OutCubic } }
              Image {
                anchors.fill: parent
                anchors.margins: 2
                source: root.frameUrl("key", key.index)
                cache: false
                asynchronous: true
                smooth: true
                mipmap: true
                sourceSize: Qt.size(144, 144)
              }
              MouseArea {
                id: keyMouse
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: root.press(key.index)
              }
            }
          }
        }

        // Stream Deck + touch strip or Neo info bar.
        Image {
          visible: !!root.device && root.device.hasScreen
          width: keys.width
          height: visible ? width / root.device.screenAspect : 0
          anchors.horizontalCenter: parent.horizontalCenter
          source: visible ? root.frameUrl("screen", 0) : ""
          cache: false
          asynchronous: true
          fillMode: Image.PreserveAspectFit
        }

        Dropdown {
          visible: !!root.device && root.profiles.length > 0
          width: parent.width
          label: strings.t("profile")
          value: root.device ? root.device.profile : ""
          options: root.profiles
          fontFamily: root.fontFamily
          onChanged: function(v) { root.selectProfile(v) }
        }

        // Pages: previous, where we are, next; Back inside a folder.
        Row {
          visible: !!root.device
          width: parent.width
          spacing: Style.space(8)
          PanelActionButton {
            iconText: "‹"
            tooltipText: root.device && root.device.inFolder ? strings.t("back") : strings.t("previousPage")
            foreground: root.foreground
            fontFamily: root.fontFamily
            onClicked: root.device && root.device.inFolder ? root.closeFolder() : root.pageStep(-1)
          }
          Text {
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width - Style.space(80)
            horizontalAlignment: Text.AlignHCenter
            color: root.dim
            font.family: root.fontFamily
            font.pixelSize: Style.font.caption
            textFormat: Text.PlainText
            text: !root.device ? "" : root.device.inFolder ? strings.t("folder") : strings.t("pageOf").arg(root.device.pageIndex + 1).arg(root.device.pageCount)
          }
          PanelActionButton {
            iconText: "›"
            tooltipText: strings.t("nextPage")
            foreground: root.foreground
            fontFamily: root.fontFamily
            enabled: !!root.device && root.device.pageCount > 1
            onClicked: root.pageStep(1)
          }
        }

        PanelSectionHeader {
          visible: !!root.device
          text: strings.t("brightness").arg(root.device ? root.device.brightness : 0)
          foreground: root.foreground
          fontFamily: root.fontFamily
        }
        PanelSlider {
          visible: !!root.device
          width: parent.width
          bar: root.bar
          minimum: 0
          maximum: 100
          step: 5
          integer: true
          value: root.device ? root.device.brightness : 0
          onReleased: function(v) { root.setBrightness(v) }
        }

        Row {
          visible: !!root.device
          spacing: Style.space(10)
          ToggleSwitch {
            anchors.verticalCenter: parent.verticalCenter
            checked: !!root.device && root.device.asleep
            foreground: root.foreground
            onToggled: root.toggleSleep()
          }
          Text {
            anchors.verticalCenter: parent.verticalCenter
            text: strings.t("sleep")
            color: root.foreground
            font.family: root.fontFamily
            font.pixelSize: Style.font.body
            textFormat: Text.PlainText
          }
        }

        Text {
          visible: !root.serviceUp
          width: parent.width
          wrapMode: Text.WordWrap
          color: root.dim
          font.family: root.fontFamily
          font.pixelSize: Style.font.caption
          textFormat: Text.PlainText
          text: strings.t("serviceHint")
        }

        Row {
          spacing: Style.space(8)
          Button {
            visible: !root.serviceUp
            text: strings.t("startService")
            onClicked: root.startService()
          }
          Button {
            text: strings.t("openEditor")
            onClicked: root.openEditor()
          }
          Button {
            visible: root.serviceUp
            text: strings.t("virtualDeck")
            onClicked: root.openVirtual()
          }
        }
      }
    }
  }
}
