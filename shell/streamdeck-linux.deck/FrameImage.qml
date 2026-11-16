import QtQuick

// A frame read from disk that keeps showing its last good picture while the
// next one loads. Two Images take turns: the hidden one loads the new source
// and is swapped in once it is ready, so a refresh never blanks the frame. A
// failed load leaves the old picture up.
Item {
  id: frame
  property url source
  property size sourceSize
  property int fillMode: Image.Stretch
  property bool mipmap: false

  property int front: 0
  readonly property Image back: front === 0 ? second : first

  onSourceChanged: load()
  Component.onCompleted: load()

  function load() {
    if (frame.source.toString() === "") {
      first.source = ""
      second.source = ""
      return
    }
    back.source = frame.source
  }

  function swapIn(image, index) {
    if (image === frame.back && image.source.toString() === frame.source.toString()) frame.front = index
  }

  Image {
    id: first
    anchors.fill: parent
    visible: frame.front === 0
    cache: false
    asynchronous: true
    sourceSize: frame.sourceSize
    fillMode: frame.fillMode
    smooth: frame.smooth
    mipmap: frame.mipmap
    onStatusChanged: if (status === Image.Ready) frame.swapIn(first, 0)
  }

  Image {
    id: second
    anchors.fill: parent
    visible: frame.front === 1
    cache: false
    asynchronous: true
    sourceSize: frame.sourceSize
    fillMode: frame.fillMode
    smooth: frame.smooth
    mipmap: frame.mipmap
    onStatusChanged: if (status === Image.Ready) frame.swapIn(second, 1)
  }
}
