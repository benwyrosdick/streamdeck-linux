.pragma library

// Pure helpers for the Stream Deck bar widget, kept free of QML so they can
// be tested with Node (tests/deck.test.mjs).

// Bounds for anything read off the control socket before it reaches the UI.
var MAX_NAME = 64
var MAX_GRID = 16

function clampText(s, n) {
  s = typeof s === "string" ? s : ""
  n = n || MAX_NAME
  return s.length > n ? s.slice(0, n - 1) + "…" : s
}

function clampInt(v, lo, hi, fallback) {
  var n = Number(v)
  if (!isFinite(n)) return fallback
  n = Math.round(n)
  return n < lo ? lo : n > hi ? hi : n
}

// The service names frame folders after the device id with anything but
// letters, digits, '-' and '_' replaced, so ids never escape the folder.
function sanitizeId(id) {
  return String(id || "").replace(/[^A-Za-z0-9_-]/g, "_")
}

// Normalises one device entry from getState.
function device(d) {
  if (!d || typeof d !== "object") return null
  return {
    id: String(d.id || ""),
    name: clampText(d.name),
    modelName: clampText(d.modelName),
    isVirtual: d["virtual"] === true,
    columns: clampInt(d.columns, 1, MAX_GRID, 5),
    rows: clampInt(d.rows, 1, MAX_GRID, 3),
    encoders: clampInt(d.encoders, 0, 8, 0),
    hasScreen: Array.isArray(d.screen) && d.screen.length === 2,
    screenAspect: Array.isArray(d.screen) && d.screen[1] > 0 ? d.screen[0] / d.screen[1] : 8,
    profile: String(d.profile || ""),
    profileName: clampText(d.profileName),
    pageIndex: clampInt(d.pageIndex, 0, 999, 0),
    pageCount: clampInt(d.pageCount, 1, 999, 1),
    inFolder: d.inFolder === true,
    asleep: d.asleep === true,
    brightness: clampInt(d.brightness, 0, 100, 60),
    smartProfile: d.smartProfile === true
  }
}

function devices(state) {
  var list = state && Array.isArray(state.devices) ? state.devices : []
  var out = []
  for (var i = 0; i < list.length && i < 16; i++) {
    var d = device(list[i])
    if (d && d.id) out.push(d)
  }
  return out
}

// The device the panel shows: the chosen one if still connected, else the
// first physical deck, else the first of any.
function pick(list, wanted) {
  for (var i = 0; i < list.length; i++) if (list[i].id === wanted) return list[i]
  for (var j = 0; j < list.length; j++) if (!list[j].isVirtual) return list[j]
  return list.length ? list[0] : null
}

function keyPosition(index, columns) {
  return (index % columns) + "," + Math.floor(index / columns)
}

function framePath(runtimeDir, deviceId, target, index) {
  return runtimeDir + "/frames/" + sanitizeId(deviceId) + "/" + target + "-" + index + ".png"
}

// Text for the bar label: the profile, shortened.
function barLabel(d, maxLen) {
  if (!d) return ""
  return clampText(d.profileName, maxLen || 14)
}

function profileOptions(list) {
  var out = []
  if (!Array.isArray(list)) return out
  for (var i = 0; i < list.length && i < 200; i++) {
    var p = list[i]
    if (!p || typeof p.id !== "string") continue
    out.push({ value: p.id, label: clampText(p.name) })
  }
  return out
}

// One request line for the control socket.
function requestLine(id, method, params) {
  return JSON.stringify({ id: id, method: method, params: params || {} }) + "\n"
}

// Parses one line from the socket into {kind: "event"|"reply"|"bad", …}.
function parseLine(line) {
  var msg
  try {
    msg = JSON.parse(line)
  } catch (e) {
    return { kind: "bad" }
  }
  if (!msg || typeof msg !== "object") return { kind: "bad" }
  if (typeof msg.event === "string") return { kind: "event", name: msg.event, data: msg.data }
  if (msg.id !== undefined) return { kind: "reply", id: msg.id, result: msg.result, error: typeof msg.error === "string" ? msg.error : "" }
  return { kind: "bad" }
}

// Events after which key images are worth re-reading.
function redrawsKeys(name) {
  return name === "state" || name === "keyEvent" || name === "profileChanged" || name === "settingsChanged" || name === "profilesChanged"
}
