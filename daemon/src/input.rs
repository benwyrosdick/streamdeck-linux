//! Synthetic keyboard and mouse input. Key combos, media keys and the mouse
//! wheel go through a uinput device: it works under every Wayland compositor
//! and needs only the `uaccess` ACL logind already grants the seat user on
//! /dev/uinput. Arbitrary Unicode text goes through `wtype`, which speaks the
//! virtual-keyboard protocol and so is not limited to the keymap.

use anyhow::{anyhow, bail, Result};
use evdev::uinput::VirtualDevice;
use evdev::{AttributeSet, EventType, InputEvent, KeyCode, RelativeAxisCode};
use serde_json::Value;
use std::sync::mpsc;
use std::sync::OnceLock;
use std::time::Duration;

pub mod keys {
    pub const ESC: u16 = 1;
    pub const BACKSPACE: u16 = 14;
    pub const TAB: u16 = 15;
    pub const ENTER: u16 = 28;
    pub const LEFTCTRL: u16 = 29;
    pub const LEFTSHIFT: u16 = 42;
    pub const LEFTALT: u16 = 56;
    pub const SPACE: u16 = 57;
    pub const LEFTMETA: u16 = 125;
    pub const INSERT: u16 = 110;
    pub const V: u16 = 47;
    pub const MUTE: u16 = 113;
    pub const VOLUMEDOWN: u16 = 114;
    pub const VOLUMEUP: u16 = 115;
    pub const NEXTSONG: u16 = 163;
    pub const PLAYPAUSE: u16 = 164;
    pub const PREVIOUSSONG: u16 = 165;
    pub const STOPCD: u16 = 166;
    pub const EJECTCD: u16 = 161;
    pub const REWIND: u16 = 168;
    pub const FASTFORWARD: u16 = 208;
    pub const SWITCHVIDEOMODE: u16 = 227;
    pub const KBDILLUMTOGGLE: u16 = 228;
    pub const KBDILLUMDOWN: u16 = 229;
    pub const KBDILLUMUP: u16 = 230;
    pub const BRIGHTNESSDOWN: u16 = 224;
    pub const BRIGHTNESSUP: u16 = 225;
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Mods {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Combo {
    pub mods: Mods,
    pub key: u16,
}

enum Cmd {
    Tap(Vec<Combo>, mpsc::Sender<Result<()>>),
    Press(Combo),
    Release(Combo),
    Wheel { vertical: i32, horizontal: i32 },
}

fn worker() -> Result<&'static mpsc::Sender<Cmd>> {
    static TX: OnceLock<std::result::Result<mpsc::Sender<Cmd>, String>> = OnceLock::new();
    TX.get_or_init(|| {
        let mut keys = AttributeSet::<KeyCode>::new();
        for code in 1..=248u16 {
            keys.insert(KeyCode(code));
        }
        let mut rel = AttributeSet::<RelativeAxisCode>::new();
        rel.insert(RelativeAxisCode::REL_WHEEL);
        rel.insert(RelativeAxisCode::REL_HWHEEL);
        rel.insert(RelativeAxisCode::REL_WHEEL_HI_RES);
        rel.insert(RelativeAxisCode::REL_HWHEEL_HI_RES);
        let device = VirtualDevice::builder()
            .and_then(|b| b.name("Stream Deck Linux virtual input").with_keys(&keys))
            .and_then(|b| b.with_relative_axes(&rel))
            .and_then(|b| b.build())
            .map_err(|e| format!("cannot create a uinput device ({e}); hotkeys need write access to /dev/uinput"))?;
        let (tx, rx) = mpsc::channel::<Cmd>();
        std::thread::Builder::new()
            .name("uinput".into())
            .spawn(move || {
                let mut dev = device;
                // Compositors ignore input from a device they have not finished
                // enumerating; the first events after creation would be lost.
                std::thread::sleep(Duration::from_millis(250));
                for cmd in rx {
                    match cmd {
                        Cmd::Tap(combos, done) => {
                            let mut res = Ok(());
                            for c in combos {
                                if let Err(e) = press(&mut dev, &c).and_then(|_| {
                                    std::thread::sleep(Duration::from_millis(12));
                                    release(&mut dev, &c)
                                }) {
                                    res = Err(anyhow!("uinput: {e}"));
                                }
                                std::thread::sleep(Duration::from_millis(12));
                            }
                            done.send(res).ok();
                        }
                        Cmd::Press(c) => {
                            press(&mut dev, &c).ok();
                        }
                        Cmd::Release(c) => {
                            release(&mut dev, &c).ok();
                        }
                        Cmd::Wheel { vertical, horizontal } => {
                            let mut ev = Vec::new();
                            if vertical != 0 {
                                ev.push(InputEvent::new(EventType::RELATIVE.0, RelativeAxisCode::REL_WHEEL.0, vertical));
                                ev.push(InputEvent::new(EventType::RELATIVE.0, RelativeAxisCode::REL_WHEEL_HI_RES.0, vertical * 120));
                            }
                            if horizontal != 0 {
                                ev.push(InputEvent::new(EventType::RELATIVE.0, RelativeAxisCode::REL_HWHEEL.0, horizontal));
                                ev.push(InputEvent::new(EventType::RELATIVE.0, RelativeAxisCode::REL_HWHEEL_HI_RES.0, horizontal * 120));
                            }
                            dev.emit(&ev).ok();
                        }
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(tx)
    })
    .as_ref()
    .map_err(|e| anyhow!("{e}"))
}

fn mod_codes(m: &Mods) -> Vec<u16> {
    let mut v = Vec::new();
    if m.ctrl {
        v.push(keys::LEFTCTRL);
    }
    if m.shift {
        v.push(keys::LEFTSHIFT);
    }
    if m.alt {
        v.push(keys::LEFTALT);
    }
    if m.meta {
        v.push(keys::LEFTMETA);
    }
    v
}

fn key_event(code: u16, value: i32) -> InputEvent {
    InputEvent::new(EventType::KEY.0, code, value)
}

fn press(dev: &mut VirtualDevice, c: &Combo) -> std::io::Result<()> {
    for m in mod_codes(&c.mods) {
        dev.emit(&[key_event(m, 1)])?;
    }
    if c.key != 0 {
        dev.emit(&[key_event(c.key, 1)])?;
    }
    Ok(())
}

fn release(dev: &mut VirtualDevice, c: &Combo) -> std::io::Result<()> {
    if c.key != 0 {
        dev.emit(&[key_event(c.key, 0)])?;
    }
    for m in mod_codes(&c.mods).into_iter().rev() {
        dev.emit(&[key_event(m, 0)])?;
    }
    Ok(())
}

/// Presses and releases each combo in order.
pub async fn tap(combos: Vec<Combo>) -> Result<()> {
    if combos.is_empty() {
        return Ok(());
    }
    let tx = worker()?;
    let (done_tx, done_rx) = mpsc::channel();
    tx.send(Cmd::Tap(combos, done_tx)).map_err(|_| anyhow!("input worker stopped"))?;
    tokio::task::spawn_blocking(move || done_rx.recv_timeout(Duration::from_secs(5)).unwrap_or(Ok(()))).await?
}

pub fn hold(combo: Combo) -> Result<()> {
    worker()?.send(Cmd::Press(combo)).map_err(|_| anyhow!("input worker stopped"))
}

pub fn unhold(combo: Combo) -> Result<()> {
    worker()?.send(Cmd::Release(combo)).map_err(|_| anyhow!("input worker stopped"))
}

pub fn wheel(vertical: i32, horizontal: i32) -> Result<()> {
    worker()?.send(Cmd::Wheel { vertical, horizontal }).map_err(|_| anyhow!("input worker stopped"))
}

pub fn available() -> std::result::Result<(), String> {
    worker().map(|_| ()).map_err(|e| e.to_string())
}

/// Types text with `wtype`, which handles characters the keymap lacks.
pub async fn type_text(text: &str) -> Result<()> {
    let status = tokio::process::Command::new("wtype").arg("--").arg(text).status().await.map_err(|e| anyhow!("wtype is needed to type text: {e}"))?;
    if !status.success() {
        bail!("wtype exited with {status}");
    }
    Ok(())
}

/// Puts text on the clipboard and sends the paste shortcut the focused
/// window expects: terminals take Ctrl+Shift+V, everything else Ctrl+V.
pub async fn paste_text(text: &str) -> Result<()> {
    use tokio::io::AsyncWriteExt;
    let mut child = tokio::process::Command::new("wl-copy").stdin(std::process::Stdio::piped()).spawn().map_err(|e| anyhow!("wl-copy is needed to paste text: {e}"))?;
    child.stdin.take().unwrap().write_all(text.as_bytes()).await?;
    child.wait().await?;
    tokio::time::sleep(Duration::from_millis(60)).await;
    let terminal = crate::system::active_window_class().await.map(|c| is_terminal(&c)).unwrap_or(false);
    let combo = Combo { mods: Mods { ctrl: true, shift: terminal, ..Default::default() }, key: keys::V };
    tap(vec![combo]).await
}

fn is_terminal(class: &str) -> bool {
    let c = class.to_ascii_lowercase();
    ["alacritty", "kitty", "foot", "ghostty", "wezterm", "konsole", "gnome-terminal", "xterm", "terminator", "tilix"].iter().any(|t| c.contains(t))
}

// ---------------------------------------------------------------------------
// Hotkey settings as the original app stores them: up to four combos, each
// with Qt key code and modifier flags.
// ---------------------------------------------------------------------------

const QT_KEY_UNKNOWN: i64 = 0x01ff_ffff;

pub fn combos_from_settings(settings: &Value) -> Vec<Combo> {
    let Some(list) = settings.get("Hotkeys").and_then(|h| h.as_array()) else { return Vec::new() };
    list.iter().filter_map(combo_from_json).collect()
}

pub fn combo_from_json(h: &Value) -> Option<Combo> {
    let qt = h.get("QTKeyCode").and_then(|v| v.as_i64()).unwrap_or(QT_KEY_UNKNOWN);
    let flag = |k: &str| h.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
    let bits = h.get("KeyModifiers").and_then(|v| v.as_i64()).unwrap_or(0);
    let mut mods = Mods {
        shift: flag("KeyShift") || bits & 1 != 0,
        ctrl: flag("KeyCtrl") || bits & 2 != 0,
        alt: flag("KeyOption") || bits & 4 != 0,
        meta: flag("KeyCmd") || bits & 8 != 0,
    };
    // A Linux-native key code stored by this port's editor wins over the Qt code.
    let native = h.get("LinuxKeyCode").and_then(|v| v.as_i64()).filter(|c| *c > 0);
    if qt == QT_KEY_UNKNOWN && native.is_none() {
        // A combo of modifiers alone is still a valid hotkey.
        return (mods != Mods::default()).then_some(Combo { mods, key: 0 });
    }
    let key = match native {
        Some(c) => c as u16,
        None => {
            let (code, needs_shift) = qt_to_evdev(qt)?;
            mods.shift |= needs_shift;
            code
        }
    };
    Some(Combo { mods, key })
}

/// Maps a `Qt::Key` value to an evdev key code on a US layout. The second
/// value says whether Shift is needed to produce that character.
pub fn qt_to_evdev(qt: i64) -> Option<(u16, bool)> {
    let ascii = |c: u8| -> Option<(u16, bool)> {
        const LETTERS: [u16; 26] = [30, 48, 46, 32, 18, 33, 34, 35, 23, 36, 37, 38, 50, 49, 24, 25, 16, 19, 31, 20, 22, 47, 17, 45, 21, 44];
        Some(match c {
            b'a'..=b'z' => (LETTERS[(c - b'a') as usize], false),
            b'A'..=b'Z' => (LETTERS[(c - b'A') as usize], false),
            b'1'..=b'9' => ((c - b'1') as u16 + 2, false),
            b'0' => (11, false),
            b' ' => (57, false),
            b'-' => (12, false),
            b'=' => (13, false),
            b'[' => (26, false),
            b']' => (27, false),
            b';' => (39, false),
            b'\'' => (40, false),
            b'`' => (41, false),
            b'\\' => (43, false),
            b',' => (51, false),
            b'.' => (52, false),
            b'/' => (53, false),
            b'!' => (2, true),
            b'@' => (3, true),
            b'#' => (4, true),
            b'$' => (5, true),
            b'%' => (6, true),
            b'^' => (7, true),
            b'&' => (8, true),
            b'*' => (9, true),
            b'(' => (10, true),
            b')' => (11, true),
            b'_' => (12, true),
            b'+' => (13, true),
            b'{' => (26, true),
            b'}' => (27, true),
            b':' => (39, true),
            b'"' => (40, true),
            b'~' => (41, true),
            b'|' => (43, true),
            b'<' => (51, true),
            b'>' => (52, true),
            b'?' => (53, true),
            _ => return None,
        })
    };
    if (0x20..0x7f).contains(&qt) {
        return ascii(qt as u8);
    }
    let code: u16 = match qt {
        0x0100_0000 => 1,   // Escape
        0x0100_0001 => 15,  // Tab
        0x0100_0002 => 15,  // Backtab
        0x0100_0003 => 14,  // Backspace
        0x0100_0004 => 28,  // Return
        0x0100_0005 => 96,  // Enter (keypad)
        0x0100_0006 => 110, // Insert
        0x0100_0007 => 111, // Delete
        0x0100_0008 => 119, // Pause
        0x0100_0009 => 99,  // Print
        0x0100_000a => 99,  // SysReq
        0x0100_0010 => 102, // Home
        0x0100_0011 => 107, // End
        0x0100_0012 => 105, // Left
        0x0100_0013 => 103, // Up
        0x0100_0014 => 106, // Right
        0x0100_0015 => 108, // Down
        0x0100_0016 => 104, // PageUp
        0x0100_0017 => 109, // PageDown
        0x0100_0020 => 42,  // Shift
        0x0100_0021 => 29,  // Control
        0x0100_0022 => 125, // Meta
        0x0100_0023 => 56,  // Alt
        0x0100_0024 => 58,  // CapsLock
        0x0100_0025 => 69,  // NumLock
        0x0100_0026 => 70,  // ScrollLock
        0x0100_0055 => 127, // Menu
        0x0100_0070 => keys::VOLUMEDOWN,
        0x0100_0071 => keys::MUTE,
        0x0100_0072 => keys::VOLUMEUP,
        0x0100_0080 => keys::PLAYPAUSE,
        0x0100_0081 => keys::STOPCD,
        0x0100_0082 => keys::PREVIOUSSONG,
        0x0100_0083 => keys::NEXTSONG,
        0x0100_0030..=0x0100_0039 => 59 + (qt - 0x0100_0030) as u16, // F1–F10
        0x0100_003a => 87,                                           // F11
        0x0100_003b => 88,                                           // F12
        0x0100_003c..=0x0100_0047 => 183 + (qt - 0x0100_003c) as u16, // F13–F24
        _ => return None,
    };
    Some((code, false))
}

/// Human-readable label for a combo, used as the default key title.
pub fn describe(c: &Combo) -> String {
    let mut parts = Vec::new();
    if c.mods.meta {
        parts.push("Super".to_string());
    }
    if c.mods.ctrl {
        parts.push("Ctrl".to_string());
    }
    if c.mods.alt {
        parts.push("Alt".to_string());
    }
    if c.mods.shift {
        parts.push("Shift".to_string());
    }
    if c.key != 0 {
        parts.push(format!("#{}", c.key));
    }
    parts.join("+")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mac_screenshot_hotkey() {
        // Cmd+Shift+4 from the original app's default profile.
        let h = serde_json::json!({"KeyCmd":true,"KeyCtrl":false,"KeyModifiers":9,"KeyOption":false,"KeyShift":true,"NativeCode":21,"QTKeyCode":52,"VKeyCode":21});
        let c = combo_from_json(&h).unwrap();
        assert_eq!(c.key, 5);
        assert!(c.mods.meta && c.mods.shift && !c.mods.ctrl);
        let empty = serde_json::json!({"KeyCmd":false,"KeyCtrl":false,"KeyModifiers":0,"KeyOption":false,"KeyShift":false,"NativeCode":-1,"QTKeyCode":33554431,"VKeyCode":-1});
        assert!(combo_from_json(&empty).is_none());
    }
}
