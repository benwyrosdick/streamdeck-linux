#!/usr/bin/env python3
"""Generates resources/builtin: manifests and icons for the built-in actions.

The action UUIDs match the original app's so profiles made there keep
working; names, descriptions and all artwork are this project's own.
Run from the repository root: python3 tools/gen_builtin.py
"""
import json
import pathlib
import shutil

ROOT = pathlib.Path(__file__).resolve().parent.parent / "resources" / "builtin"

# Glyphs are drawn on a 100x100 grid with round strokes.
G = {
    "keyboard": '<rect x="12" y="28" width="76" height="44" rx="8"/><path d="M24 42h4M38 42h4M52 42h4M66 42h4M24 56h4M66 56h4M36 58h28"/>',
    "keyboard-switch": '<rect x="12" y="34" width="76" height="40" rx="8"/><path d="M26 48h4M42 48h4M58 48h4M30 62h40"/><path d="M30 16h40l-8-8M70 24H30l8 8"/>',
    "open": '<path d="M16 30v46a6 6 0 0 0 6 6h56a6 6 0 0 0 6-6V40a6 6 0 0 0-6-6H48l-8-10H22a6 6 0 0 0-6 6z"/>',
    "app": '<rect x="18" y="18" width="26" height="26" rx="6"/><rect x="56" y="18" width="26" height="26" rx="6"/><rect x="18" y="56" width="26" height="26" rx="6"/><path d="M69 58v22M58 69h22"/>',
    "close": '<rect x="16" y="16" width="68" height="68" rx="12"/><path d="M36 36l28 28M64 36L36 64"/>',
    "website": '<circle cx="50" cy="50" r="34"/><path d="M16 50h68M50 16c-12 12-12 56 0 68M50 16c12 12 12 56 0 68"/>',
    "text": '<path d="M24 24h52M50 24v56M38 80h24"/>',
    "media": '<path d="M38 24v38"/><circle cx="30" cy="66" r="10"/><path d="M38 24l40-8v40"/><circle cx="70" cy="60" r="10"/>',
    "brightness": '<circle cx="50" cy="50" r="14"/><path d="M50 14v10M50 76v10M14 50h10M76 50h10M25 25l7 7M68 68l7 7M25 75l7-7M68 32l7-7"/>',
    "wheel": '<rect x="28" y="12" width="44" height="76" rx="22"/><path d="M50 26v16"/><path d="M40 62l10 10 10-10"/>',
    "sleep": '<path d="M70 62A30 30 0 1 1 40 22a24 24 0 0 0 30 40z"/><path d="M62 18h14l-14 14h14"/>',
    "timer": '<circle cx="50" cy="54" r="30"/><path d="M50 54V38M42 14h16M50 14v10M74 26l6-6"/>',
    "play-audio": '<path d="M18 40h14l20-16v52L32 60H18z"/><path d="M64 36a20 20 0 0 1 0 28M72 26a34 34 0 0 1 0 48"/>',
    "stop-audio": '<path d="M18 40h14l20-16v52L32 60H18z"/><path d="M64 40l20 20M84 40L64 60"/>',
    "multi": '<rect x="14" y="14" width="44" height="44" rx="8"/><path d="M68 32h12a6 6 0 0 1 6 6v36a6 6 0 0 1-6 6H44a6 6 0 0 1-6-6V68"/>',
    "multi-switch": '<rect x="14" y="14" width="44" height="44" rx="8"/><path d="M68 32h12a6 6 0 0 1 6 6v36a6 6 0 0 1-6 6H44a6 6 0 0 1-6-6V68"/><path d="M26 36h20M40 30l6 6-6 6"/>',
    "random": '<rect x="16" y="16" width="68" height="68" rx="12"/><circle cx="34" cy="34" r="4" fill="#fff"/><circle cx="66" cy="34" r="4" fill="#fff"/><circle cx="50" cy="50" r="4" fill="#fff"/><circle cx="34" cy="66" r="4" fill="#fff"/><circle cx="66" cy="66" r="4" fill="#fff"/>',
    "delay": '<path d="M30 14h40M30 86h40M34 14c0 22 32 22 32 36S34 64 34 86M66 86c0-10-16-16-16-16s-16 6-16 16"/>',
    "page-prev": '<rect x="14" y="22" width="72" height="56" rx="8"/><path d="M56 36L42 50l14 14"/>',
    "page-next": '<rect x="14" y="22" width="72" height="56" rx="8"/><path d="M44 36l14 14-14 14"/>',
    "page-goto": '<rect x="14" y="22" width="72" height="56" rx="8"/><path d="M32 50h34M54 38l12 12-12 12"/>',
    "page-indicator": '<rect x="14" y="22" width="72" height="56" rx="8"/><circle cx="36" cy="66" r="3"/><circle cx="50" cy="66" r="3"/><circle cx="64" cy="66" r="3"/>',
    "folder": '<path d="M14 30v44a6 6 0 0 0 6 6h60a6 6 0 0 0 6-6V40a6 6 0 0 0-6-6H48l-8-10H20a6 6 0 0 0-6 6z"/><path d="M50 46v20M40 56h20"/>',
    "parent": '<path d="M14 30v44a6 6 0 0 0 6 6h60a6 6 0 0 0 6-6V40a6 6 0 0 0-6-6H48l-8-10H20a6 6 0 0 0-6 6z"/><path d="M58 66H42V50M42 50l-8 8M42 50l8 8"/>',
    "profile": '<rect x="14" y="20" width="56" height="56" rx="8"/><rect x="30" y="30" width="56" height="56" rx="8"/><path d="M48 58h20M62 52l6 6-6 6"/>',
    "vsd": '<rect x="10" y="20" width="80" height="50" rx="6"/><path d="M36 84h28M50 70v14"/><path d="M22 32h10v10H22zM45 32h10v10H45zM68 32h10v10H68zM22 50h10v10H22zM45 50h10v10H45zM68 50h10v10H68z"/>',
    "action-bar": '<rect x="10" y="30" width="80" height="40" rx="8"/><path d="M22 50h8M38 50h8M54 50h8M70 50h8"/>',
    "key-logic": '<rect x="14" y="14" width="72" height="72" rx="12"/><path d="M34 40v20M34 40h8M58 34v32M70 34v32"/>',
    "action-wheel": '<circle cx="50" cy="50" r="34"/><circle cx="50" cy="50" r="10"/><path d="M50 16v12M50 72v12M16 50h12M72 50h12"/>',
    "action-trigger": '<circle cx="50" cy="50" r="22"/><path d="M22 30a36 36 0 0 0 0 40M78 30a36 36 0 0 1 0 40M16 36l6-6 6 6M84 64l-6 6-6-6"/>',
    "dial-stack": '<circle cx="50" cy="40" r="22"/><path d="M24 64a30 14 0 0 0 52 0M22 76a32 14 0 0 0 56 0"/>',
    "clock": '<circle cx="50" cy="50" r="34"/><path d="M50 30v20l14 8"/>',
    "dots": '<circle cx="30" cy="50" r="6"/><circle cx="50" cy="50" r="6"/><circle cx="70" cy="50" r="6"/>',
    # Multimedia variants
    "media-previous": '<path d="M28 26v48M76 26L38 50l38 24z"/>',
    "media-playpause": '<path d="M22 24l32 26-32 26z"/><path d="M66 26v48M80 26v48"/>',
    "media-next": '<path d="M72 26v48M24 26l38 24-38 24z"/>',
    "media-stop": '<rect x="26" y="26" width="48" height="48" rx="6"/>',
    "media-mute": '<path d="M16 40h14l20-16v52L30 60H16z"/><path d="M62 40l20 20M82 40L62 60"/>',
    "media-volume-up": '<path d="M16 40h14l20-16v52L30 60H16z"/><path d="M62 50h20M72 40v20"/>',
    "media-volume-down": '<path d="M16 40h14l20-16v52L30 60H16z"/><path d="M62 50h20"/>',
    "media-volume": '<path d="M16 40h14l20-16v52L30 60H16z"/><path d="M62 36a20 20 0 0 1 0 28M70 26a34 34 0 0 1 0 48"/>',
    "media-brightness-up": '<circle cx="50" cy="50" r="14"/><path d="M50 14v10M50 76v10M14 50h10M76 50h10M25 25l7 7M68 68l7 7M25 75l7-7M68 32l7-7"/>',
    "media-brightness-down": '<circle cx="50" cy="50" r="12"/><path d="M50 26v4M50 70v4M26 50h4M70 50h4"/>',
    "media-brightness": '<circle cx="50" cy="50" r="14"/><path d="M50 14v10M50 76v10M14 50h10M76 50h10"/>',
    "media-eject": '<path d="M50 22L22 58h56z"/><path d="M22 74h56"/>',
    "media-mirror": '<rect x="10" y="26" width="36" height="28" rx="4"/><rect x="54" y="26" width="36" height="28" rx="4"/><path d="M28 70h44M40 62l-12 8 12 8M60 62l12 8-12 8"/>',
    "media-fast-forward": '<path d="M16 28l30 22-30 22zM50 28l30 22-30 22z"/>',
    "media-rewind": '<path d="M84 28L54 50l30 22zM50 28L20 50l30 22z"/>',
    "media-kbd-up": '<rect x="12" y="44" width="76" height="34" rx="6"/><path d="M50 14v18M42 22l8-8 8 8"/>',
    "media-kbd-down": '<rect x="12" y="44" width="76" height="34" rx="6"/><path d="M50 14v18M42 24l8 8 8-8"/>',
    "media-kbd-toggle": '<rect x="12" y="44" width="76" height="34" rx="6"/><path d="M30 18l6 8M50 14v10M70 18l-6 8"/>',
    "media-kbd-brightness": '<rect x="12" y="44" width="76" height="34" rx="6"/><path d="M30 18l6 8M50 14v10M70 18l-6 8"/>',
    "media-player": '<circle cx="50" cy="50" r="34"/><path d="M42 36l22 14-22 14z"/>',
    # Brightness variants
    "brightness-increase": '<circle cx="50" cy="50" r="14"/><path d="M50 14v10M50 76v10M14 50h10M76 50h10M25 25l7 7M68 68l7 7M25 75l7-7M68 32l7-7"/><path d="M80 14v14M73 21h14"/>',
    "brightness-decrease": '<circle cx="50" cy="50" r="14"/><path d="M50 14v10M50 76v10M14 50h10M76 50h10M25 25l7 7M68 68l7 7M25 75l7-7M68 32l7-7"/><path d="M73 21h14"/>',
    "brightness-max": '<circle cx="50" cy="50" r="16" fill="#fff"/><path d="M50 12v12M50 76v12M12 50h12M76 50h12M23 23l9 9M68 68l9 9M23 77l9-9M68 32l9-9"/>',
    "brightness-high": '<circle cx="50" cy="50" r="15"/><path d="M50 18v8M50 74v8M18 50h8M74 50h8M28 28l6 6M66 66l6 6M28 72l6-6M66 34l6-6"/>',
    "brightness-medium": '<circle cx="50" cy="50" r="15"/><path d="M50 22v6M50 72v6M22 50h6M72 50h6"/>',
    "brightness-low": '<circle cx="50" cy="50" r="15"/><path d="M50 26v4M50 70v4M26 50h4M70 50h4"/>',
    "brightness-min": '<circle cx="50" cy="50" r="15"/>',
}

ACCENTS = {
    "nav": ("#2b5cff", "#1a2f80"),
    "system": ("#3b3f4a", "#1c1e24"),
    "multi": ("#8a3cff", "#3d1a73"),
    "audio": ("#00a37a", "#00513d"),
    "keys": ("#ff7a1a", "#7a3300"),
}


def key_svg(glyph, accent):
    a, b = ACCENTS[accent]
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 144 144" width="144" height="144">'
        f'<defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{a}"/><stop offset="1" stop-color="{b}"/></linearGradient></defs>'
        '<rect width="144" height="144" fill="url(#g)"/>'
        '<g transform="translate(32 26) scale(0.8)" fill="none" stroke="#fff" stroke-width="7" stroke-linecap="round" stroke-linejoin="round">'
        f"{G[glyph]}</g></svg>"
    )


def list_svg(glyph):
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" width="40" height="40">'
        '<g fill="none" stroke="#e6e6e6" stroke-width="7" stroke-linecap="round" stroke-linejoin="round">'
        f"{G[glyph]}</g></svg>"
    )


KP = ["Keypad"]
KE = ["Keypad", "Encoder"]
EN = ["Encoder"]

# plugin id, name, category, accent, actions
# action: uuid, name, tooltip, glyph, controllers, states, extras
PLUGINS = [
    ("com.elgato.streamdeck.system.hotkey", "Hotkey", "System", "keys", [
        ("com.elgato.streamdeck.system.hotkey", "Hotkey", "Press a key combination; on a dial, rotate or push to send one", "keyboard", KE, 1, {"Encoder": {"TriggerDescription": {"Rotate": "Send left/right hotkey", "Push": "Send push hotkey", "Touch": "Send touch hotkey"}}}),
    ]),
    ("com.elgato.streamdeck.system.hotkeyswitch", "Hotkey Switch", "System", "keys", [
        ("com.elgato.streamdeck.system.hotkeyswitch", "Hotkey Switch", "Alternate between two key combinations", "keyboard-switch", KP, 2, {"SupportedInMultiActions": False}),
    ]),
    ("com.elgato.streamdeck.system.open", "Open", "System", "system", [
        ("com.elgato.streamdeck.system.open", "Open", "Open a file, folder, program or command", "open", KP, 1, {}),
    ]),
    ("com.elgato.streamdeck.system.openapp", "Open Application", "System", "system", [
        ("com.elgato.streamdeck.system.openapp", "Open Application", "Open or focus an application; hold to quit it", "app", KP, 1, {}),
    ]),
    ("com.elgato.streamdeck.system.close", "Close", "System", "system", [
        ("com.elgato.streamdeck.system.close", "Close", "Close every window of an application", "close", KP, 1, {}),
    ]),
    ("com.elgato.streamdeck.system.website", "Website", "System", "system", [
        ("com.elgato.streamdeck.system.website", "Website", "Open a web page, or call a URL in the background", "website", KP, 1, {}),
    ]),
    ("com.elgato.streamdeck.system.text", "Text", "System", "system", [
        ("com.elgato.streamdeck.system.text", "Text", "Type or paste a piece of text", "text", KP, 1, {}),
    ]),
    ("com.elgato.streamdeck.system.multimedia", "Multimedia", "System", "system", [
        ("com.elgato.streamdeck.system.multimedia", "Multimedia", "Media keys: play, skip, volume, brightness", "media", KE, 1, {"Encoder": {"TriggerDescription": {"Rotate": "Adjust", "Push": "Toggle"}}}),
    ]),
    ("com.elgato.streamdeck.system.keybrightness", "Brightness", "Stream Deck", "nav", [
        ("com.elgato.streamdeck.system.keybrightness", "Brightness", "Change the Stream Deck's brightness", "brightness", KE, 1, {"Encoder": {"TriggerDescription": {"Rotate": "Adjust brightness", "Push": "Dim / full"}}}),
    ]),
    ("com.elgato.streamdeck.system.mouse", "Mouse", "System", "system", [
        ("com.elgato.streamdeck.system.mouse-wheel", "Mouse Wheel", "Scroll with a dial", "wheel", EN, 1, {"Encoder": {"TriggerDescription": {"Rotate": "Scroll"}}}),
    ]),
    ("com.elgato.streamdeck.system.sleep", "Sleep", "Stream Deck", "nav", [
        ("com.elgato.streamdeck.system.sleep", "Sleep", "Turn the Stream Deck's keys off until the next press", "sleep", KP, 1, {}),
    ]),
    ("com.elgato.streamdeck.timer", "Timer", "System", "system", [
        ("com.elgato.streamdeck.system.timer", "Timer", "A countdown on the key: press to start or pause, hold to reset", "timer", KE, 1, {"SupportedInMultiActions": False}),
    ]),
    ("com.elgato.streamdeck.soundboard", "Soundboard", "Soundboard", "audio", [
        ("com.elgato.streamdeck.soundboard.playaudio", "Play Audio", "Play a sound file, or a random one from a folder", "play-audio", KE, 1, {}),
        ("com.elgato.streamdeck.soundboard.stopaudioplay", "Stop Audio", "Stop everything the soundboard is playing", "stop-audio", KE, 1, {}),
    ]),
    ("com.elgato.streamdeck.multiactions", "Multi Action", "Multi Action", "multi", [
        ("com.elgato.streamdeck.multiactions.routine", "Multi Action", "Run several actions in a row", "multi", KP, 1, {"SupportedInMultiActions": False, "SupportedInKeyLogicActions": False}),
        ("com.elgato.streamdeck.multiactions.routine2", "Multi Action Switch", "Alternate between two lists of actions", "multi-switch", KP, 2, {"SupportedInMultiActions": False, "SupportedInKeyLogicActions": False}),
        ("com.elgato.streamdeck.multiactions.random", "Random Action", "Run one action picked at random", "random", KP, 1, {"SupportedInMultiActions": False, "SupportedInKeyLogicActions": False}),
        ("com.elgato.streamdeck.multiactions.delay", "Delay", "Wait before the next action in a multi action", "delay", KP, 1, {"VisibleInActionsList": False}),
    ]),
    ("com.elgato.streamdeck.page", "Pages", "Stream Deck", "nav", [
        ("com.elgato.streamdeck.page.previous", "Previous Page", "Go to the previous page", "page-prev", KE, 1, {"SupportedInMultiActions": False}),
        ("com.elgato.streamdeck.page.next", "Next Page", "Go to the next page", "page-next", KE, 1, {"SupportedInMultiActions": False}),
        ("com.elgato.streamdeck.page.goto", "Go to Page", "Jump to a page of this profile", "page-goto", KP, 1, {"SupportedInMultiActions": False}),
        ("com.elgato.streamdeck.page.indicator", "Page Indicator", "Show the current page; press for the next", "page-indicator", KE, 1, {"SupportedInMultiActions": False}),
    ]),
    ("com.elgato.streamdeck.profile.openchild", "Create Folder", "Stream Deck", "nav", [
        ("com.elgato.streamdeck.profile.openchild", "Create Folder", "A key that opens a folder of more keys", "folder", KP, 1, {"SupportedInMultiActions": False, "SupportedInKeyLogicActions": False}),
    ]),
    ("com.elgato.streamdeck.profile.backtoparent", "Parent Folder", "Stream Deck", "nav", [
        ("com.elgato.streamdeck.profile.backtoparent", "Parent Folder", "Leave the folder", "parent", KP, 1, {"SupportedInMultiActions": False, "VisibleInActionsList": False}),
    ]),
    ("com.elgato.streamdeck.profile.rotate", "Switch Profile", "Stream Deck", "nav", [
        ("com.elgato.streamdeck.profile.rotate", "Switch Profile", "Change to another profile", "profile", KE, 1, {}),
    ]),
    ("com.elgato.streamdeck.vsdtoggle", "Virtual Stream Deck", "Stream Deck", "nav", [
        ("com.elgato.streamdeck.system.vsdtoggle", "Toggle Virtual Stream Deck", "Show or hide the Virtual Stream Deck", "vsd", KP, 1, {}),
    ]),
    ("com.elgato.streamdeck.system.actionbar", "Action Bar", "Stream Deck", "nav", [
        ("com.elgato.streamdeck.system.actionbar", "Action Bar", "Show or hide the Virtual Stream Deck's action bar", "action-bar", KP, 2, {}),
    ]),
    ("com.elgato.streamdeck.keys", "Keys", "Stream Deck", "multi", [
        ("com.elgato.streamdeck.keys.logic", "Key Logic", "Different actions for press, double press and hold", "key-logic", KP, 1, {"SupportedInMultiActions": False, "SupportedInKeyLogicActions": False}),
        ("com.elgato.streamdeck.keys.stack", "Action Wheel", "Several key actions on one dial: rotate to choose, push to run", "action-wheel", EN, 1, {"Encoder": {"TriggerDescription": {"Rotate": "Choose action", "Push": "Run action"}}}),
        ("com.elgato.streamdeck.keys.adaptor", "Action Trigger", "Run key actions by turning, pushing or touching a dial", "action-trigger", EN, 1, {"Encoder": {"TriggerDescription": {"Rotate": "Run left/right action", "Push": "Run push action", "Touch": "Run touch action"}}}),
    ]),
    ("com.elgato.streamdeck.dial", "Dials", "Stream Deck", "multi", [
        ("com.elgato.streamdeck.dial.stack", "Dial Stack", "Several dial actions on one dial; push to switch", "dial-stack", EN, 1, {"Encoder": {"TriggerDescription": {"Push": "Next action"}}}),
    ]),
    ("com.elgato.streamdeck.system.digitaltime", "Digital Time", "Stream Deck", "nav", [
        ("com.elgato.streamdeck.system.digitaltime", "Digital Time", "Clock on the Stream Deck Neo's info bar", "clock", ["Neo"], 1, {}),
    ]),
    ("com.elgato.streamdeck.system.pagination", "Page Indicator (Neo)", "Stream Deck", "nav", [
        ("com.elgato.streamdeck.system.pagination", "Page Indicator", "Page dots on the Stream Deck Neo's info bar", "dots", ["Neo"], 1, {}),
    ]),
]

EXTRA_KEY_IMAGES = [g for g in G if g.startswith("media-") or g.startswith("brightness-")]


def main():
    if ROOT.exists():
        shutil.rmtree(ROOT)
    for pid, pname, category, accent, actions in PLUGINS:
        d = ROOT / f"{pid}.sdPlugin"
        (d / "Images").mkdir(parents=True)
        manifest = {
            "UUID": pid,
            "Name": pname,
            "Author": "streamdeck-linux",
            "Description": pname,
            "Category": category,
            "CategoryIcon": "Images/category",
            "Icon": f"Images/{actions[0][3]}-list",
            "Version": "1.0",
            "SDKVersion": 2,
            "PrivateAPI": True,
            "OS": [{"Platform": "linux", "MinimumVersion": "6"}],
            "Software": {"MinimumVersion": "6.0"},
            "Actions": [],
        }
        (d / "Images" / "category.svg").write_text(list_svg(actions[0][3]))
        for uuid, name, tip, glyph, ctl, nstates, extra in actions:
            (d / "Images" / f"{glyph}.svg").write_text(key_svg(glyph, accent))
            (d / "Images" / f"{glyph}-list.svg").write_text(list_svg(glyph))
            states = [{"Image": f"Images/{glyph}", "TitleAlignment": "bottom", "FontSize": 12} for _ in range(nstates)]
            entry = {"UUID": uuid, "Name": name, "Tooltip": tip, "Icon": f"Images/{glyph}-list", "Controllers": ctl, "States": states}
            if "Encoder" in ctl:
                enc = {"layout": "$X1", "Icon": f"Images/{glyph}-list"}
                enc.update(extra.get("Encoder", {}))
                entry["Encoder"] = enc
            for k, v in extra.items():
                if k != "Encoder":
                    entry[k] = v
            manifest["Actions"].append(entry)
        if pid.endswith("multimedia") or pid.endswith("keybrightness"):
            for g in EXTRA_KEY_IMAGES:
                (d / "Images" / f"{g}.svg").write_text(key_svg(g, accent))
        (d / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"wrote {len(PLUGINS)} built-in plugins to {ROOT}")


if __name__ == "__main__":
    main()
