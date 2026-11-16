# streamdeckd control API

The editor, the Virtual Stream Deck, the Omarchy bar widget and the
`streamdeckd ctl` command all talk to the service over one Unix socket:

```
$XDG_RUNTIME_DIR/streamdeck-linux/control.sock     (override: STREAMDECK_RUNTIME_DIR)
```

The protocol is newline-delimited JSON. A request is one line:

```json
{"id": 7, "method": "selectProfile", "params": {"device": "A00SA4092MJEJI", "profile": "…"}}
```

and gets exactly one reply line with the same id:

```json
{"id": 7, "result": true}
{"id": 7, "error": "unknown profile"}
```

Replies can interleave with events on a subscribed connection; match on `id`.

From a shell: `streamdeckd ctl METHOD '{"param": …}'`.

## Events

Send `subscribe` once; afterwards the connection receives lines of the form
`{"event": NAME, "data": …}`:

| event | data | when |
|---|---|---|
| `state` | same as `getState` | device connected/disconnected, profile or page changed, brightness, sleep |
| `profilesChanged` | `{}` | profiles created, deleted, renamed, imported |
| `profileChanged` | `{profile, page?}` | an action or page of a profile was edited |
| `settingsChanged` | `{context}` | a plugin or property inspector changed an action's settings |
| `pluginsChanged` | `{}` | plugin installed, removed, or (re)connected |
| `pluginCrashed` | `{plugin}` | a plugin was stopped after crashing repeatedly |
| `keyEvent` | `{device, controller, position, down}` | any key press/release |
| `activeApp` | `{class}` | focused window changed (Hyprland) |
| `alert` | `{context, message}` | an action failed; the key shows a warning triangle |
| `toggleVirtualDeck` | `{}` | the *Toggle Virtual Stream Deck* action was pressed |
| `toggleActionBar` | `{}` | the *Action Bar* action was pressed |

`subscribeFrames` (optional `{device}`) streams rendered images instead:
`{"event": "frame", "data": {"device", "target": "key"|"screen", "index", "png": <base64>}}`.
Current frames are replayed immediately after subscribing.

Every frame is also written to
`$XDG_RUNTIME_DIR/streamdeck-linux/frames/<device>/key-<index>.png` (and
`screen-0.png` for the Plus touch strip / Neo info bar), replaced atomically.

## Concepts

- **device**: id is the serial number for hardware, `virtual-…` for a
  Virtual Stream Deck.
- **profile**: UUID. Stored as `~/.local/share/streamdeck-linux/ProfilesV2/<UUID>.sdProfile`,
  the same layout the original app uses.
- **page**: UUID. A profile lists its top-level pages in order; every other
  page is a *folder* opened by a Create Folder action.
- **controller**: `Keypad`, `Encoder` (Stream Deck + dials) or `Neo` (info bar).
- **position**: `"column,row"` for keys, `"index,0"` for dials.
- **path** (optional): descends into child actions of containers (multi
  action, key logic, action wheel, dial stack, action trigger): a list of
  `[routine, index]` pairs.
- **context**: an action's `ActionID`; the SDK context plugins see.

## Methods

### State and devices
| method | params | result |
|---|---|---|
| `ping` | | `"pong"` |
| `getState` | | `{devices: [{id, name, model, modelName, virtual, serial, firmware, columns, rows, encoders, touchPoints, screen: [w,h]\|null, profile, profileName, page, pageIndex, pageCount, inFolder, asleep, brightness, sleepMinutes, smartProfile}], sdkPort, activeApp}` |
| `getFrames` | `device` | `[{target, index, png}]` |
| `press` | `device, position, controller?, down?` | omit `down` for a full press+release |
| `dial` | `device, index, ticks? \| down? \| touch?, long?` | |
| `setBrightness` | `device, value` (0–100) | |
| `setSleep` | `device, minutes` (0 = never) | |
| `sleep` / `wake` | `device` | |
| `renameDevice` | `device, name` | |
| `addVirtualDeck` | `name?, columns, rows` | new device id |
| `removeVirtualDeck` | `id` | |

### Profiles and pages
| method | params | result |
|---|---|---|
| `listProfiles` | `device?` | `[{id, name, model, device, app, pages, currentPage, defaultPage}]` |
| `getProfile` | `profile` | `{summary, manifest, pages: {<page>: PageData + _dir + _folder}, dir}` |
| `createProfile` | `device, name?` | profile id |
| `deleteProfile` / `duplicateProfile` | `profile` (`name?`) | |
| `renameProfile` | `profile, name` | |
| `setProfileApp` | `profile, app` (window class or .desktop id; empty clears) | smart profile binding |
| `selectProfile` | `device, profile, page?` | also becomes the device's remembered profile |
| `importProfile` | `path, device?` | new profile ids (`.streamDeckProfile` zip or `.sdProfile` dir; v1 profiles are converted) |
| `exportProfile` | `profile, path` | |
| `addPage` | `profile, folder?` | page id |
| `deletePage` / `renamePage` / `setDefaultPage` | `profile, page` (`name`) | |
| `movePage` | `profile, page, index` | |
| `selectPage` | `device, page` | pages and folders |
| `pageStep` | `device, delta` | |
| `closeFolder` | `device` | |

`PageData` is the on-disk page manifest: `{Name, Icon, Controllers: [{Type, Actions: {"c,r": ActionData}}]}`.
`ActionData` is `{ActionID, UUID, Name, Settings, State, States: [{Title, ShowTitle, TitleAlignment, TitleColor, FontFamily, FontSize, FontStyle, FontUnderline, OutlineThickness, Image}], LinkedTitle, Plugin?, Actions?: [{Actions: [ActionData]}], Encoder?, Resources?}`.

### Actions
| method | params | result |
|---|---|---|
| `listPlugins` | | `[{uuid, name, version, author, description, category, categoryIcon, icon, dir, builtin, running, unsupported, actions: [{uuid, plugin, name, tooltip, icon, propertyInspector, states, controllers, supportedInMultiActions, supportedInKeyLogic, visible, encoder}]}]` |
| `setAction` | loc + `uuid` | new ActionData placed at loc (replaces) |
| `updateAction` | loc + `action` (full ActionData) | saved; plugin gets `didReceiveSettings` |
| `removeAction` | loc | |
| `moveAction` | loc + `to: loc`, `copy?` | moving onto an occupied key swaps |
| `setActionImage` | loc + `state?`, `path` \| `data` (base64 PNG) \| neither (clears), `target?: "background"` | |
| `setActionState` | `context, state` | |
| `getAction` | `context` | `{action, loc, trigger}` |
| `piInfo` | `context` | `{port, uuid, registerEvent, info, actionInfo, path, builtin, pluginRunning, unsupported}`: everything needed to call `connectElgatoStreamDeckSocket` in a property inspector |

`loc` = `{profile, page, controller, position, path?}`.

### Plugins, icons, backups, settings
| method | params | result |
|---|---|---|
| `installPlugin` | `path` (`.streamDeckPlugin` or `.sdPlugin` dir) | plugin uuid |
| `uninstallPlugin` / `restartPlugin` | `uuid` | |
| `pluginLog` | `uuid` | last 20 kB of its log |
| `listIconPacks` | | `[{id, name, author, version, icon, icons: [{path, name, tags}]}]` |
| `installIconPack` | `path` | id |
| `removeIconPack` | `id` | |
| `listApplications` | | desktop entries: `[{id, name, exec, icon, iconPath, path, wmClass}]` |
| `listFonts` | | font family names |
| `backup` | `path?` | written file |
| `listBackups` | | `[{path, name, size, modified}]` |
| `restoreBackup` | `path` | the current profiles are backed up first |
| `getSettings` / `setSettings` | partial settings object | `{sdkPort, language, smartProfiles, autoBackup, backupsToKeep, pressFeedback, appAliases, …}` |
| `deepLink` | `url` (`streamdeck://plugins/message/<uuid>/…`) | forwarded as `didReceiveDeepLink` |
| `reload` | | re-reads profiles and plugins from disk |
