# Built-in actions

Every built-in action of Stream Deck 7.6 exists here under the same UUID, so
profiles exported from the macOS or Windows app import and keep working. The
**Settings** column lists the keys stored in the profile. Keys marked *(orig)*
are the original app's (seen in its exported profiles); unmarked keys are this
port's own, used where the original never exposes its stored format.

| Action | UUID | Settings | Behaviour on Linux |
|---|---|---|---|
| Hotkey | `com.elgato.streamdeck.system.hotkey` | `Hotkeys` *(orig)*: four entries `{QTKeyCode, KeyShift, KeyCtrl, KeyOption, KeyCmd, KeyModifiers, …}`; `LinuxKeyCode` added when recorded here | Key: slot 1 is held while the key is held (push-to-talk works). Dial: slots are rotate left, rotate right, push, touch. Sent through a uinput keyboard, so it works in every Wayland app. Cmd is Super. |
| Hotkey Switch | `…system.hotkeyswitch` | `Hotkeys` *(orig)*: slot 1 for state 1, slot 2 for state 2 | Sends the current state's combination, then flips state. |
| Open | `…system.open` | `path` *(orig)* | Files and folders open with `xdg-open`; executables and `.desktop` files launch; anything else runs as a shell command. macOS `.app` paths show a warning. |
| Open Application | `…system.openapp` | `bundle_path` (desktop file), `app_name`, `bring_to_front`, `long_press` (`quit`, `force_quit`, `none`) | Focuses the app if it has a window (Hyprland), otherwise launches it. Holding quits it. A green dot shows it is running. |
| Close | `…system.close` | `bundle_path`, `app_name` | Closes every window of the app; holding force-quits. |
| Website | `…system.website` | `path`, `openInBrowser`, `browser` *(orig)* | Opens in the default browser, or requests the URL in the background (check mark on success). |
| Text | `…system.text` | `pastedText`, `isSendingEnter`, `isTypingMode` *(orig)* | Pastes via the clipboard (Ctrl+Shift+V in terminals) or types with `wtype`; optionally presses Enter. |
| Multimedia | `…system.multimedia` | `actionIdx` *(orig)* | Sends the media key (play, next, volume, brightness, keyboard backlight, eject, display mode), so Omarchy shows its usual OSD. On a dial, turning adjusts and pushing toggles. Display contrast (9, 10, 20) has no Linux equivalent. |
| Brightness | `…system.keybrightness` | `actionIdx` *(orig)*: 0 increase, 1 decrease, 2 max, 3 high, 4 medium, 5 low, 6 min | The Stream Deck's own brightness; a dial adjusts in 5 % steps. |
| Mouse Wheel | `…system.mouse-wheel` | `direction` (`vertical`/`horizontal`), `invert`, `step` | Dial only; scrolls through a uinput pointer. |
| Sleep | `…system.sleep` | `DeviceUUID` *(orig)* | Turns the keys off until the next press. |
| Timer | `…system.timer` | `duration` (seconds), `sound` (file) | Press starts or pauses, hold resets. Countdown on the key with a progress ring; alert and sound at zero. |
| Play Audio | `…soundboard.playaudio` | `path` *(orig)*, `volume` (0–100), `actionType` (0 play/stop, 1 play/overlap, 2 play/restart, 3 loop/stop, 4 press & hold), `fadeType` (0 none, 1 in, 2 out, 3 both), `fadeLen` (ms) | A folder plays a random file from it. MP3, WAV, OGG, FLAC, AAC/M4A. |
| Stop Audio | `…soundboard.stopaudioplay` | | Stops all soundboard playback. |
| Multi Action | `…multiactions.routine` | `Actions[0].Actions` *(orig layout)* | Runs the steps in order, waiting for each. |
| Multi Action Switch | `…multiactions.routine2` | `Actions[0]`, `Actions[1]` | Alternates between the two lists. |
| Random Action | `…multiactions.random` | `Actions[0]` | Runs one step at random. |
| Delay | `…multiactions.delay` | `delay` (ms) | Only inside multi actions. |
| Previous / Next Page | `…page.previous`, `…page.next` | | Also on dials (turn). |
| Go to Page | `…page.goto` | `PageIndex` (1-based) | |
| Page Indicator | `…page.indicator` | | Shows `n/m`; press for the next page. |
| Create Folder | `…profile.openchild` | `ProfileUUID` *(orig)* | Opens the folder; double-click it in the editor to fill it. |
| Parent Folder | `…profile.backtoparent` | | |
| Switch Profile | `…profile.rotate` | `DeviceUUID`, `ProfileUUID` *(orig)*, `PageIndex` | Empty profile = next profile for that device. |
| Toggle Virtual Stream Deck | `…system.vsdtoggle` | | Opens or closes the Virtual Stream Deck window. |
| Action Bar | `…system.actionbar` | | Two states; tells the editor to toggle its action bar. |
| Key Logic | `…keys.logic` | `Actions[0]` press, `Actions[1]` double press, `Actions[2]` hold; `doublePressMs` | A hold is 500 ms. |
| Action Wheel | `…keys.stack` | `Actions[0]` (key actions) | Dial: turn to choose (shown on the strip), push to run. |
| Action Trigger | `…keys.adaptor` | `Actions[0..3]`: rotate left, rotate right, push, touch | Dial: runs key actions from dial gestures. |
| Dial Stack | `…dial.stack` | `Actions[0]` (dial actions) | Push moves to the next dial action; everything else goes to the current one. |
| Digital Time | `…system.digitaltime` | `24HourFormat` *(orig)* | Clock on the Stream Deck Neo info bar. |
| Page Indicator (Neo) | `…system.pagination` | | Page dots on the Neo info bar (shown by default). |

## Stream Deck + touch strip

Plugins choose a layout with `setFeedbackLayout` and fill it with
`setFeedback`. The SDK's built-in layouts `$X1`, `$A0`, `$A1`, `$B1`, `$B2`
and `$C1` are implemented, as are custom layout JSON files inside a plugin
(items of type `pixmap`, `text`, `bar` and `gbar` with `rect`, `value`,
`font`, `alignment`, `color`, `background`, `opacity`, `zOrder`, `range`,
`bar_fill_c`, `bar_bg_c`, `bar_border_c`, `border_w`, `bar_h`). Swiping the
strip turns pages.

## Stream Deck Neo

The info bar shows the time and page dots; the two touch points turn pages.
