pragma Singleton
import QtQuick

// Editor state shared by every view: which device and profile are open,
// what is selected, the action catalogue, and the undo history. Everything
// persistent lives in streamdeckd; this only mirrors it.
QtObject {
    id: store

    readonly property bool connected: daemon.connected
    readonly property var devices: (daemon.state && daemon.state.devices) ? daemon.state.devices : []
    property string deviceId: ""
    readonly property var device: {
        for (const d of devices)
            if (d.id === deviceId) return d
        return null
    }
    readonly property string profileId: device ? (device.profile || "") : ""
    readonly property string pageId: device ? (device.page || "") : ""

    property var profiles: []
    property var profile: null          // getProfile result for profileId
    property var plugins: []
    property var actionIndex: ({})      // action uuid -> action definition
    property var pluginIndex: ({})      // plugin uuid -> plugin
    property var applications: []
    property var fonts: []

    // {controller: "Keypad"|"Encoder"|"Neo", position: "c,r", path: [[routine, index], …]}
    property var selected: null
    property var clipboard: null
    property var undoStack: []
    property var redoStack: []
    property int revision: 0            // bumps whenever profile data reloads

    signal toast(string text, bool error)
    signal frameChanged(string device, string target, int index, int revision)

    // ------------------------------------------------------------------
    // Daemon plumbing

    function call(method, params, cb) {
        daemon.call(method, params || {}, function (result, error) {
            if (error && error.length) {
                if (cb) cb(null, error)
                else toast(error, true)
                return
            }
            if (cb) cb(result, "")
        })
    }

    property Connections daemonEvents: Connections {
        target: daemon
        function onEvent(name, data) {
            switch (name) {
            case "profilesChanged":
                store.refreshProfiles()
                store.refreshProfile()
                break
            case "profileChanged":
                if (!data || data.profile === store.profileId) store.refreshProfile()
                break
            case "settingsChanged":
                store.refreshProfile()
                break
            case "pluginsChanged":
                store.refreshPlugins()
                break
            case "pluginCrashed":
                store.toast(qsTr("Plugin %1 keeps crashing and was stopped.").arg(data.plugin), true)
                break
            case "alert":
                if (data && data.message) store.toast(data.message, true)
                break
            }
        }
        function onStateChanged() {
            if (!store.device && store.devices.length > 0) {
                const want = startDevice || util.env("STREAMDECK_TEST_DEVICE")
                const wanted = want && store.devices.find(d => d.id === want)
                const hw = store.devices.find(d => !d.virtual)
                store.deviceId = (wanted || hw || store.devices[0]).id
            }
        }
        function onConnectedChanged() {
            if (daemon.connected) {
                store.refreshPlugins()
                store.refreshProfiles()
                store.refreshProfile()
                store.call("listFonts", {}, function (r) { if (r) store.fonts = r })
            }
        }
        function onFrameChanged(device, target, index, revision) {
            store.frameChanged(device, target, index, revision)
        }
    }

    // The service may already be connected when the singleton is created.
    Component.onCompleted: {
        if (daemon.connected) daemonEvents.onConnectedChanged()
        // Screenshot and UI tests preselect a key: STREAMDECK_TEST_SELECT=Keypad/3,0
        const pre = util.env("STREAMDECK_TEST_SELECT")
        if (pre) {
            const parts = pre.split("/")
            selected = { controller: parts[0], position: parts[1], path: parts[2] ? JSON.parse(parts[2]) : [] }
        }
    }

    property bool _first: true
    onProfileIdChanged: { if (!_first) selected = null; refreshProfile(); refreshProfiles() }
    onDeviceIdChanged: refreshProfiles()
    onPageIdChanged: if (!_first) selected = null

    function refreshProfiles() {
        if (!deviceId) { profiles = []; return }
        call("listProfiles", { device: deviceId }, function (r) { if (r) profiles = r })
    }

    function refreshProfile() {
        if (!profileId) { profile = null; return }
        const id = profileId
        call("getProfile", { profile: id }, function (r) {
            if (r && id === store.profileId) {
                store.profile = r
                store.revision++
                store._first = false
            }
        })
    }

    function refreshPlugins() {
        call("listPlugins", {}, function (r) {
            if (!r) return
            const ai = {}, pi = {}
            for (const p of r) {
                pi[p.uuid] = p
                for (const a of p.actions) ai[a.uuid] = Object.assign({ pluginName: p.name, builtin: p.builtin, category: p.category || p.name }, a)
            }
            store.plugins = r
            store.pluginIndex = pi
            store.actionIndex = ai
        })
    }

    function loadApplications(cb) {
        if (applications.length) { if (cb) cb(applications); return }
        call("listApplications", {}, function (r) { if (r) { applications = r; if (cb) cb(r) } })
    }

    // ------------------------------------------------------------------
    // Reading the open profile

    function page(id) {
        if (!profile || !profile.pages) return null
        return profile.pages[id || pageId] || null
    }

    function controller(pageData, kind) {
        if (!pageData || !pageData.Controllers) return null
        for (const c of pageData.Controllers)
            if (c.Type === kind) return c
        return null
    }

    function actionAt(sel) {
        if (!sel) return null
        const c = controller(page(), sel.controller)
        let a = c && c.Actions ? c.Actions[sel.position] : null
        for (const step of (sel.path || [])) {
            if (!a || !a.Actions || !a.Actions[step[0]]) return null
            a = a.Actions[step[0]].Actions[step[1]] || null
        }
        return a
    }

    function selectedAction() {
        revision // re-evaluate bindings after reloads
        return actionAt(selected)
    }

    function loc(sel) {
        return { profile: profileId, page: pageId, controller: sel.controller, position: sel.position, path: sel.path || [] }
    }

    function definition(uuid) {
        return actionIndex[uuid] || null
    }

    function sameSel(a, b) {
        return !!a && !!b && a.controller === b.controller && a.position === b.position && JSON.stringify(a.path || []) === JSON.stringify(b.path || [])
    }

    function isFolderPage(id) {
        const p = page(id)
        return !!p && !!p._folder
    }

    // ------------------------------------------------------------------
    // Editing, with undo. Each history entry restores whole actions.

    function _record(entries) {
        undoStack = undoStack.concat([entries]).slice(-100)
        redoStack = []
    }

    function _apply(entries, cb) {
        // entries: [{loc, action|null}]
        let pending = entries.length
        for (const e of entries) {
            if (e.action) call("updateAction", Object.assign({}, e.loc, { action: e.action }), function () { if (--pending === 0 && cb) cb() })
            else call("removeAction", e.loc, function () { if (--pending === 0 && cb) cb() })
        }
    }

    function _snapshot(locs) {
        return locs.map(l => ({ loc: l, action: JSON.parse(JSON.stringify(actionAtLoc(l) || null)) }))
    }

    function actionAtLoc(l) {
        if (!profile || l.profile !== profileId) return null
        return actionAt({ controller: l.controller, position: l.position, path: l.path })
    }

    function undo() {
        if (!undoStack.length) return
        const entries = undoStack[undoStack.length - 1]
        undoStack = undoStack.slice(0, -1)
        const now = _snapshot(entries.map(e => e.loc))
        redoStack = redoStack.concat([now])
        _apply(entries)
    }

    function redo() {
        if (!redoStack.length) return
        const entries = redoStack[redoStack.length - 1]
        redoStack = redoStack.slice(0, -1)
        undoStack = undoStack.concat([_snapshot(entries.map(e => e.loc))])
        _apply(entries)
    }

    function placeAction(sel, uuid, cb) {
        const l = loc(sel)
        _record(_snapshot([l]))
        call("setAction", Object.assign({}, l, { uuid: uuid }), function (r, err) {
            if (err) { toast(err, true); return }
            selected = { controller: sel.controller, position: sel.position, path: sel.path || [] }
            if (cb) cb(r)
        })
    }

    // Adds an action to a container's routine (multi action steps, key logic slots…).
    function addChild(parentSel, routine, uuid) {
        const parent = actionAt(parentSel)
        if (!parent) return
        const count = parent.Actions && parent.Actions[routine] ? parent.Actions[routine].Actions.length : 0
        const path = (parentSel.path || []).concat([[routine, count]])
        const l = loc(parentSel)
        _record(_snapshot([l]))
        call("setAction", Object.assign({}, l, { path: path, uuid: uuid }), function (r, err) {
            if (err) toast(err, true)
        })
    }

    function updateAction(sel, action, cb) {
        const l = loc(sel)
        _record(_snapshot([l]))
        call("updateAction", Object.assign({}, l, { action: action }), function (r, err) {
            if (err) toast(err, true)
            if (cb) cb(r)
        })
    }

    // Shallow-merges settings and saves; the usual path for inspectors.
    function updateSettings(sel, patch) {
        const a = actionAt(sel)
        if (!a) return
        const copy = JSON.parse(JSON.stringify(a))
        copy.Settings = Object.assign({}, copy.Settings || {}, patch)
        updateAction(sel, copy)
    }

    function updateState(sel, stateIndex, patch) {
        const a = actionAt(sel)
        if (!a) return
        const copy = JSON.parse(JSON.stringify(a))
        copy.States = copy.States || []
        while (copy.States.length <= stateIndex) copy.States.push({})
        copy.States[stateIndex] = Object.assign({}, copy.States[stateIndex], patch)
        updateAction(sel, copy)
    }

    function removeAction(sel) {
        if (!actionAt(sel)) return
        const l = loc(sel)
        _record(_snapshot([l]))
        call("removeAction", l)
        if (sameSel(sel, selected)) selected = (sel.path && sel.path.length) ? { controller: sel.controller, position: sel.position, path: sel.path.slice(0, -1) } : null
    }

    function moveAction(from, to, copy) {
        if (sameSel(from, to)) return
        const lf = loc(from), lt = loc(to)
        _record(_snapshot(copy ? [lt] : [lf, lt]))
        call("moveAction", Object.assign({}, lf, { to: lt, copy: !!copy }), function (r, err) {
            if (err) { toast(err, true); return }
            selected = to
        })
    }

    function copySelected() {
        const a = actionAt(selected)
        if (a) clipboard = JSON.parse(JSON.stringify(a))
    }

    function pasteTo(sel) {
        if (!clipboard || !sel) return
        const a = JSON.parse(JSON.stringify(clipboard))
        // Pasted keys are new actions with their own contexts.
        const renew = function (x) {
            x.ActionID = util.uuid()
            for (const r of (x.Actions || [])) for (const c of r.Actions) renew(c)
        }
        renew(a)
        updateAction(sel, a, function () { selected = sel })
    }

    function setImage(sel, stateIndex, path, target) {
        const l = loc(sel)
        _record(_snapshot([l]))
        const params = Object.assign({}, l, { state: stateIndex })
        if (path) params.path = path
        if (target) params.target = target
        call("setActionImage", params)
    }

    function press(sel) {
        if (!device || !sel) return
        call("press", { device: deviceId, controller: sel.controller, position: sel.position })
    }

    function selectPage(id) {
        call("selectPage", { device: deviceId, page: id })
    }

    function selectProfile(id) {
        call("selectProfile", { device: deviceId, profile: id })
    }
}
