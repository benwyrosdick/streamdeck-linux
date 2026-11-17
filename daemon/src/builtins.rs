//! Built-in actions. Their manifests (names, icons, controllers) live in
//! `resources/builtin`; their behaviour lives here.
//!
//! Settings keys follow the original app where profiles exported from it
//! show them (Hotkey, Open, Website, Text, Multimedia, Brightness, folders,
//! Switch Profile, Soundboard path/volume). Keys for actions whose stored
//! settings the original never exposes in a shipped profile (Timer, Delay,
//! Key Logic timings, Mouse Wheel) are this port's own and documented in
//! docs/ACTIONS.md.

use crate::audio;
use crate::engine::{work, DialEv, Engine, Msg, Work};
use crate::input::{self, Combo, Mods};
use crate::render::{ImageSource, Overlay};
use crate::system;
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::sync::oneshot;
use uuid::Uuid;

pub const HOTKEY: &str = "com.elgato.streamdeck.system.hotkey";
pub const HOTKEY_SWITCH: &str = "com.elgato.streamdeck.system.hotkeyswitch";
pub const OPEN: &str = "com.elgato.streamdeck.system.open";
pub const OPEN_APP: &str = "com.elgato.streamdeck.system.openapp";
pub const CLOSE: &str = "com.elgato.streamdeck.system.close";
pub const WEBSITE: &str = "com.elgato.streamdeck.system.website";
pub const TEXT: &str = "com.elgato.streamdeck.system.text";
pub const MULTIMEDIA: &str = "com.elgato.streamdeck.system.multimedia";
pub const BRIGHTNESS: &str = "com.elgato.streamdeck.system.keybrightness";
pub const MOUSE_WHEEL: &str = "com.elgato.streamdeck.system.mouse-wheel";
pub const SLEEP: &str = "com.elgato.streamdeck.system.sleep";
pub const TIMER: &str = "com.elgato.streamdeck.system.timer";
pub const PLAY_AUDIO: &str = "com.elgato.streamdeck.soundboard.playaudio";
pub const STOP_AUDIO: &str = "com.elgato.streamdeck.soundboard.stopaudioplay";
pub const MULTI: &str = "com.elgato.streamdeck.multiactions.routine";
pub const MULTI_SWITCH: &str = "com.elgato.streamdeck.multiactions.routine2";
pub const RANDOM: &str = "com.elgato.streamdeck.multiactions.random";
pub const DELAY: &str = "com.elgato.streamdeck.multiactions.delay";
pub const PAGE_PREV: &str = "com.elgato.streamdeck.page.previous";
pub const PAGE_NEXT: &str = "com.elgato.streamdeck.page.next";
pub const PAGE_GOTO: &str = "com.elgato.streamdeck.page.goto";
pub const PAGE_INDICATOR: &str = "com.elgato.streamdeck.page.indicator";
pub const FOLDER: &str = "com.elgato.streamdeck.profile.openchild";
pub const PARENT: &str = "com.elgato.streamdeck.profile.backtoparent";
pub const SWITCH_PROFILE: &str = "com.elgato.streamdeck.profile.rotate";
pub const VSD_TOGGLE: &str = "com.elgato.streamdeck.system.vsdtoggle";
pub const ACTION_BAR: &str = "com.elgato.streamdeck.system.actionbar";
pub const KEY_LOGIC: &str = "com.elgato.streamdeck.keys.logic";
pub const ACTION_WHEEL: &str = "com.elgato.streamdeck.keys.stack";
pub const ACTION_TRIGGER: &str = "com.elgato.streamdeck.keys.adaptor";
pub const DIAL_STACK: &str = "com.elgato.streamdeck.dial.stack";
pub const DIGITAL_TIME: &str = "com.elgato.streamdeck.system.digitaltime";
// The Neo info bar always shows its page dots, so nothing looks for this one.
#[allow(dead_code)]
pub const PAGINATION: &str = "com.elgato.streamdeck.system.pagination";
// This port's own; the original has no microphone action.
pub const MIC_MUTE: &str = "streamdeck-linux.audio.micmute";

/// `actionIdx` of the Multimedia action, in the order of the original app's
/// action list. Indices 18–22 are the dial-oriented entries.
pub fn media_key(idx: i64) -> Option<(u16, &'static str)> {
    use input::keys::*;
    Some(match idx {
        0 => (PREVIOUSSONG, "previous"),
        1 => (PLAYPAUSE, "playpause"),
        2 => (NEXTSONG, "next"),
        3 => (STOPCD, "stop"),
        4 => (MUTE, "mute"),
        5 => (VOLUMEUP, "volume-up"),
        6 => (VOLUMEDOWN, "volume-down"),
        7 => (BRIGHTNESSUP, "brightness-up"),
        8 => (BRIGHTNESSDOWN, "brightness-down"),
        9 | 10 => return None, // display contrast has no Linux equivalent
        11 => (EJECTCD, "eject"),
        12 => (SWITCHVIDEOMODE, "mirror"),
        13 => (FASTFORWARD, "fast-forward"),
        14 => (REWIND, "rewind"),
        15 => (KBDILLUMUP, "kbd-up"),
        16 => (KBDILLUMDOWN, "kbd-down"),
        17 => (KBDILLUMTOGGLE, "kbd-toggle"),
        18 => (MUTE, "volume"),
        19 => (BRIGHTNESSUP, "brightness"),
        20 => return None,
        21 => (KBDILLUMTOGGLE, "kbd-brightness"),
        22 => (PLAYPAUSE, "player"),
        _ => return None,
    })
}

/// What a dial-style media entry does on rotation: (counter-clockwise, clockwise).
fn media_rotation(idx: i64) -> Option<(u16, u16)> {
    use input::keys::*;
    match idx {
        4 | 5 | 6 | 18 => Some((VOLUMEDOWN, VOLUMEUP)),
        7 | 8 | 19 => Some((BRIGHTNESSDOWN, BRIGHTNESSUP)),
        15 | 16 | 17 | 21 => Some((KBDILLUMDOWN, KBDILLUMUP)),
        0 | 1 | 2 | 22 => Some((PREVIOUSSONG, NEXTSONG)),
        13 | 14 => Some((REWIND, FASTFORWARD)),
        _ => None,
    }
}

const BRIGHTNESS_LEVELS: [&str; 7] = ["increase", "decrease", "max", "high", "medium", "low", "min"];

fn combo(key: u16) -> Combo {
    Combo { mods: Mods::default(), key }
}

impl Engine {
    fn builtin_dir(&self, uuid: &str) -> Option<PathBuf> {
        self.registry.plugin_for_action(uuid).map(|p| p.dir.clone())
    }

    fn builtin_image(&self, uuid: &str, name: &str) -> Option<ImageSource> {
        let p = self.builtin_dir(uuid)?.join("Images").join(format!("{name}.svg"));
        p.exists().then_some(ImageSource::File(p))
    }

    fn uuid_of(&self, ctx: &str) -> String {
        self.instances.get(ctx).map(|i| i.uuid.clone()).unwrap_or_default()
    }

    fn device_of(&self, ctx: &str) -> String {
        self.instances.get(ctx).map(|i| i.device.clone()).unwrap_or_default()
    }

    pub fn builtin_appear(&mut self, ctx: &str) {
        let uuid = self.uuid_of(ctx);
        let Some(a) = self.ctx_action(ctx).cloned() else { return };
        let dyn_image = match uuid.as_str() {
            MULTIMEDIA => media_key(a.setting_i64("actionIdx", 0)).and_then(|(_, n)| self.builtin_image(&uuid, &format!("media-{n}"))),
            BRIGHTNESS => BRIGHTNESS_LEVELS.get(a.setting_i64("actionIdx", 0) as usize).and_then(|n| self.builtin_image(&uuid, &format!("brightness-{n}"))),
            OPEN_APP | CLOSE => app_from_settings(&a.settings).and_then(|app| app.icon_path.map(|p| ImageSource::File(PathBuf::from(p)))),
            OPEN => {
                let path = a.setting_str("path");
                let path = path.trim_matches('"');
                if path.ends_with(".desktop") {
                    system::find_app(path).and_then(|app| app.icon_path).map(|p| ImageSource::File(PathBuf::from(p)))
                } else {
                    None
                }
            }
            _ => None,
        };
        // A user's custom image still wins: dyn images only replace the manifest default.
        let has_custom = a.current_state().and_then(|s| s.image.as_ref()).map(|s| !s.is_empty()).unwrap_or(false);
        if let Some(inst) = self.instances.get_mut(ctx) {
            if !has_custom {
                inst.dyn_image = dyn_image;
            }
            if uuid == TIMER {
                let total = a.setting_i64("duration", 60).max(1);
                inst.rt = json!({ "total": total, "remaining_ms": total * 1000, "running": false });
                inst.dyn_title = Some(fmt_duration(total * 1000));
            }
        }
        match uuid.as_str() {
            PAGE_INDICATOR => self.update_page_indicator(ctx),
            OPEN_APP => self.update_app_running(ctx),
            ACTION_WHEEL | DIAL_STACK => self.update_stack_display(ctx),
            MIC_MUTE => self.refresh_mic(),
            _ => {}
        }
    }

    pub fn builtin_disappear(&mut self, ctx: &str) {
        if self.uuid_of(ctx) == HOTKEY {
            if let Some(held) = self.instances.get(ctx).and_then(|i| i.rt.get("held").cloned()) {
                if let Some(c) = serde_json::from_value::<(bool, bool, bool, bool, u16)>(held).ok() {
                    input::unhold(Combo { mods: Mods { shift: c.0, ctrl: c.1, alt: c.2, meta: c.3 }, key: c.4 }).ok();
                }
            }
        }
    }

    pub fn builtin_down(&mut self, ctx: &str) -> Work {
        let uuid = self.uuid_of(ctx);
        let Some(a) = self.ctx_action(ctx).cloned() else { return None };
        let nested = self.instances.get(ctx).map(|i| i.parent.is_some()).unwrap_or(false);
        let dev = self.device_of(ctx);
        if let Some(inst) = self.instances.get_mut(ctx) {
            inst.rt["down_at"] = json!(now_ms());
            inst.rt["long"] = json!(false);
        }
        match uuid.as_str() {
            HOTKEY => {
                let combos = input::combos_from_settings(&a.settings);
                let Some(first) = combos.into_iter().next() else { return Some(Box::pin(async { Err(anyhow!("no hotkey set")) })) };
                if nested {
                    return work(async move { input::tap(vec![first]).await });
                }
                // Held keys stay held while the Stream Deck key is down,
                // which makes push-to-talk shortcuts work.
                if let Some(inst) = self.instances.get_mut(ctx) {
                    inst.rt["held"] = json!((first.mods.shift, first.mods.ctrl, first.mods.alt, first.mods.meta, first.key));
                }
                if let Err(e) = input::hold(first) {
                    return Some(Box::pin(async move { Err(e) }));
                }
                None
            }
            HOTKEY_SWITCH => {
                let list = input::combos_from_settings(&a.settings);
                let c = list.get(a.state as usize).cloned().or_else(|| list.first().cloned());
                match c {
                    Some(c) => work(async move { input::tap(vec![c]).await }),
                    None => Some(Box::pin(async { Err(anyhow!("no hotkey set")) })),
                }
            }
            OPEN => {
                let path = a.setting_str("path");
                work(async move { system::open_path(&path) })
            }
            WEBSITE => {
                let url = a.setting_str("path");
                let browser = a.setting_str("browser");
                if a.setting_bool("openInBrowser", true) {
                    work(async move { system::open_url(&url, Some(&browser)) })
                } else {
                    let ctx = ctx.to_string();
                    let tx = self.tx.clone();
                    work(async move {
                        system::fetch_url(&url).await?;
                        tx.send(Msg::Exec(Box::new(move |e: &mut Engine| e.show_overlay(&ctx, Overlay::Ok)))).ok();
                        Ok(())
                    })
                }
            }
            TEXT => {
                let text = a.setting_str("pastedText");
                let enter = a.setting_bool("isSendingEnter", false);
                let typing = a.setting_bool("isTypingMode", false);
                work(async move {
                    if !text.is_empty() {
                        if typing {
                            input::type_text(&text).await?;
                        } else {
                            input::paste_text(&text).await?;
                        }
                    }
                    if enter {
                        input::tap(vec![combo(input::keys::ENTER)]).await?;
                    }
                    if text.is_empty() && !enter {
                        bail!("no text set");
                    }
                    Ok(())
                })
            }
            MULTIMEDIA => {
                let idx = a.setting_i64("actionIdx", 0);
                match media_key(idx) {
                    Some((key, _)) => work(async move { input::tap(vec![combo(key)]).await }),
                    None => Some(Box::pin(async { Err(anyhow!("display contrast is not adjustable on Linux")) })),
                }
            }
            BRIGHTNESS => {
                let idx = a.setting_i64("actionIdx", 0);
                let cur = self.settings.devices.get(&dev).map(|s| s.brightness as i64).unwrap_or(60);
                let next = match idx {
                    0 => cur + 10,
                    1 => cur - 10,
                    2 => 100,
                    3 => 75,
                    4 => 50,
                    5 => 25,
                    _ => 5,
                };
                self.set_brightness(&dev, next.clamp(0, 100) as u8);
                None
            }
            SLEEP => {
                let target = a.setting_str("DeviceUUID");
                let target = if target.is_empty() || !self.devices.contains_key(&target) { dev } else { target };
                // Sleep after release, or the release would wake it again.
                let tx = self.tx.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(300)).await;
                    tx.send(Msg::Exec(Box::new(move |e: &mut Engine| e.sleep_device(&target)))).ok();
                });
                None
            }
            PLAY_AUDIO => self.soundboard_down(ctx, &a),
            STOP_AUDIO => {
                audio::stop_all();
                None
            }
            DELAY => {
                let ms = a.setting_i64("delay", 500).max(0) as u64;
                work(async move {
                    tokio::time::sleep(Duration::from_millis(ms)).await;
                    Ok(())
                })
            }
            MULTI => self.run_routine(ctx, 0),
            MULTI_SWITCH => {
                let w = self.run_routine(ctx, a.state as usize);
                let next = (a.state + 1) % 2;
                self.set_state(ctx, next);
                w
            }
            RANDOM => {
                let n = a.actions.as_ref().and_then(|r| r.first()).map(|r| r.actions.len()).unwrap_or(0);
                if n == 0 {
                    return Some(Box::pin(async { Err(anyhow!("no actions to pick from")) }));
                }
                use rand::Rng;
                let i = rand::rng().random_range(0..n);
                self.run_children(ctx, vec![(0, i)])
            }
            PAGE_PREV => {
                self.page_step(&dev, -1);
                None
            }
            PAGE_NEXT | PAGE_INDICATOR => {
                self.page_step(&dev, 1);
                None
            }
            PAGE_GOTO => {
                let idx = a.setting_i64("PageIndex", 1);
                let pages = self.devices.get(&dev).and_then(|d| d.profile).and_then(|p| self.store.get(&p)).map(|p| p.top_pages()).unwrap_or_default();
                // PageIndex counts from 1 as the page list does in the editor.
                match pages.get((idx.max(1) - 1) as usize) {
                    Some(pg) => {
                        let pg = *pg;
                        self.switch_page(&dev, pg, true);
                        None
                    }
                    None => Some(Box::pin(async move { Err(anyhow!("this profile has no page {idx}")) })),
                }
            }
            FOLDER => {
                self.open_folder(ctx, &a.setting_str("ProfileUUID"));
                None
            }
            PARENT => {
                self.close_folder(&dev);
                None
            }
            SWITCH_PROFILE => {
                let target_dev = a.setting_str("DeviceUUID");
                let target_dev = if target_dev.is_empty() || !self.devices.contains_key(&target_dev) { dev.clone() } else { target_dev };
                let profile = a.setting_str("ProfileUUID").parse::<Uuid>().ok().filter(|p| self.store.get(p).is_some());
                let profile = match profile {
                    Some(p) => p,
                    None => match self.next_profile(&target_dev) {
                        Some(p) => p,
                        None => return Some(Box::pin(async { Err(anyhow!("no other profile for this device")) })),
                    },
                };
                let page = a.setting_i64("PageIndex", 0);
                let pages = self.store.get(&profile).map(|p| p.top_pages()).unwrap_or_default();
                let pg = if page > 0 { pages.get(page as usize - 1).copied() } else { None };
                // Remember the choice so the device comes back to it.
                self.settings.device(&target_dev).profile = Some(profile.to_string());
                self.settings.save();
                self.switch_profile(&target_dev, profile, pg);
                None
            }
            VSD_TOGGLE => {
                self.emit("toggleVirtualDeck", json!({}));
                self.toggle_vsd_window();
                None
            }
            ACTION_BAR => {
                self.emit("toggleActionBar", json!({}));
                let s = (a.state + 1) % 2;
                self.set_state(ctx, s);
                None
            }
            MIC_MUTE => {
                // A Multi Action step can ask for Muted or Live; otherwise toggle.
                let desired = self.instances.get(ctx).and_then(|i| i.rt.get("desired")).and_then(|v| v.as_u64()).map(|s| s == 1);
                let tx = self.tx.clone();
                work(async move {
                    let r = system::set_mic_muted(desired).await;
                    tx.send(Msg::Exec(Box::new(|e: &mut Engine| e.refresh_mic()))).ok();
                    r
                })
            }
            TIMER | OPEN_APP | KEY_LOGIC | CLOSE => None, // act on release / hold
            _ => None,
        }
    }

    pub fn builtin_up(&mut self, ctx: &str, held: Duration) -> Work {
        let uuid = self.uuid_of(ctx);
        let Some(a) = self.ctx_action(ctx).cloned() else { return None };
        let long = self.instances.get(ctx).and_then(|i| i.rt.get("long")).and_then(|l| l.as_bool()).unwrap_or(false);
        match uuid.as_str() {
            HOTKEY => {
                if let Some(held) = self.instances.get_mut(ctx).and_then(|i| i.rt.as_object_mut()).and_then(|o| o.remove("held")) {
                    if let Ok(c) = serde_json::from_value::<(bool, bool, bool, bool, u16)>(held) {
                        input::unhold(Combo { mods: Mods { shift: c.0, ctrl: c.1, alt: c.2, meta: c.3 }, key: c.4 }).ok();
                    }
                }
                None
            }
            HOTKEY_SWITCH => {
                let n = (a.state + 1) % 2;
                self.set_state(ctx, n);
                None
            }
            OPEN_APP if !long => {
                let settings = a.settings.clone();
                let tx = self.tx.clone();
                let ctx2 = ctx.to_string();
                work(async move {
                    let app = app_from_settings(&settings).ok_or_else(|| anyhow!("pick an application first"))?;
                    let front = settings.get("bring_to_front").and_then(|v| v.as_bool()).unwrap_or(true);
                    let wins = system::app_windows(&app).await;
                    if front && !wins.is_empty() {
                        system::focus_window(&wins[0]).await?;
                    } else {
                        system::launch_app(&app, &[])?;
                    }
                    tokio::time::sleep(Duration::from_millis(1500)).await;
                    tx.send(Msg::Exec(Box::new(move |e: &mut Engine| e.update_app_running(&ctx2)))).ok();
                    Ok(())
                })
            }
            CLOSE => {
                let settings = a.settings.clone();
                let force = long;
                work(async move {
                    let app = app_from_settings(&settings).ok_or_else(|| anyhow!("pick an application first"))?;
                    let wins = system::app_windows(&app).await;
                    if wins.is_empty() {
                        bail!("{} is not running", app.name);
                    }
                    for w in wins {
                        if force {
                            system::kill_pid(w.pid, true);
                        } else {
                            system::close_window(&w).await?;
                        }
                    }
                    Ok(())
                })
            }
            PLAY_AUDIO => {
                // Press & Hold mode stops when the key is released.
                if a.setting_i64("actionType", 0) == 4 {
                    audio::stop(ctx, Duration::from_millis(a.setting_i64("fadeLen", 0).max(0) as u64));
                }
                None
            }
            TIMER if !long => {
                let Some(inst) = self.instances.get_mut(ctx) else { return None };
                let running = inst.rt.get("running").and_then(|r| r.as_bool()).unwrap_or(false);
                let remaining = inst.rt.get("remaining_ms").and_then(|r| r.as_i64()).unwrap_or(0);
                if remaining <= 0 {
                    let total = a.setting_i64("duration", 60).max(1);
                    inst.rt["remaining_ms"] = json!(total * 1000);
                }
                inst.rt["running"] = json!(!running);
                inst.rt["last"] = json!(now_ms());
                self.render_ctx(ctx);
                None
            }
            KEY_LOGIC if !long => {
                // A second press inside the window is a double press.
                let window = a.setting_i64("doublePressMs", 300).max(100) as u64;
                let has_double = a.actions.as_ref().and_then(|r| r.get(1)).map(|r| !r.actions.is_empty()).unwrap_or(false);
                if !has_double {
                    return self.run_routine(ctx, 0);
                }
                let pending = self.instances.get(ctx).and_then(|i| i.rt.get("pending_tap")).and_then(|v| v.as_u64());
                if let Some(t) = pending {
                    if now_ms().saturating_sub(t) < window {
                        if let Some(inst) = self.instances.get_mut(ctx) {
                            inst.rt.as_object_mut().map(|o| o.remove("pending_tap"));
                        }
                        return self.run_routine(ctx, 1);
                    }
                }
                let stamp = now_ms();
                if let Some(inst) = self.instances.get_mut(ctx) {
                    inst.rt["pending_tap"] = json!(stamp);
                }
                let tx = self.tx.clone();
                let ctx2 = ctx.to_string();
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(window)).await;
                    tx.send(Msg::Exec(Box::new(move |e: &mut Engine| {
                        let still = e.instances.get(&ctx2).and_then(|i| i.rt.get("pending_tap")).and_then(|v| v.as_u64()) == Some(stamp);
                        if still {
                            if let Some(inst) = e.instances.get_mut(&ctx2) {
                                inst.rt.as_object_mut().map(|o| o.remove("pending_tap"));
                            }
                            let w = e.run_routine(&ctx2, 0);
                            e.spawn_work(&ctx2, w);
                        }
                    })))
                    .ok();
                });
                None
            }
            _ => {
                let _ = held;
                None
            }
        }
    }

    /// Fired 500 ms into a press that is still held.
    pub fn builtin_hold(&mut self, ctx: &str) {
        let uuid = self.uuid_of(ctx);
        let Some(a) = self.ctx_action(ctx).cloned() else { return };
        let handled = match uuid.as_str() {
            OPEN_APP => {
                let mode = a.setting_str("long_press");
                let mode = if mode.is_empty() { "quit".to_string() } else { mode };
                if mode == "none" {
                    false
                } else {
                    let settings = a.settings.clone();
                    let force = mode == "force_quit" || a.setting_bool("force_kill", false);
                    self.spawn_work(
                        ctx,
                        work(async move {
                            let app = app_from_settings(&settings).ok_or_else(|| anyhow!("pick an application first"))?;
                            for w in system::app_windows(&app).await {
                                if force {
                                    system::kill_pid(w.pid, true);
                                } else {
                                    system::close_window(&w).await?;
                                }
                            }
                            Ok(())
                        }),
                    );
                    true
                }
            }
            CLOSE => true,
            TIMER => {
                if let Some(inst) = self.instances.get_mut(ctx) {
                    let total = a.setting_i64("duration", 60).max(1);
                    inst.rt = json!({ "total": total, "remaining_ms": total * 1000, "running": false });
                    inst.dyn_title = Some(fmt_duration(total * 1000));
                    inst.progress = None;
                }
                self.render_ctx(ctx);
                true
            }
            KEY_LOGIC => {
                let has_hold = a.actions.as_ref().and_then(|r| r.get(2)).map(|r| !r.actions.is_empty()).unwrap_or(false);
                if has_hold {
                    let w = self.run_routine(ctx, 2);
                    self.spawn_work(ctx, w);
                }
                has_hold
            }
            _ => false,
        };
        if handled {
            if let Some(inst) = self.instances.get_mut(ctx) {
                inst.rt["long"] = json!(true);
            }
        }
    }

    pub fn builtin_dial(&mut self, ctx: &str, ev: DialEv) -> Work {
        let uuid = self.uuid_of(ctx);
        let Some(a) = self.ctx_action(ctx).cloned() else { return None };
        let dev = self.device_of(ctx);
        let enc_slot = |list: &[Combo], i: usize| list.get(i).cloned();
        match (uuid.as_str(), ev) {
            (HOTKEY, ev) => {
                let list = input::combos_from_settings(&a.settings);
                let pick = match ev {
                    DialEv::Rotate(t) if t < 0 => enc_slot(&list, 0).map(|c| vec![c; t.unsigned_abs() as usize]),
                    DialEv::Rotate(t) => enc_slot(&list, 1).map(|c| vec![c; t as usize]),
                    DialEv::Down => enc_slot(&list, 2).map(|c| vec![c]),
                    DialEv::Touch { .. } => enc_slot(&list, 3).map(|c| vec![c]),
                    DialEv::Up => None,
                };
                pick.map(|c| -> Work { work(async move { input::tap(c).await }) }).flatten()
            }
            (MULTIMEDIA, ev) => {
                let idx = a.setting_i64("actionIdx", 18);
                match ev {
                    DialEv::Rotate(t) => {
                        let (ccw, cw) = media_rotation(idx)?;
                        let key = if t < 0 { ccw } else { cw };
                        let n = t.unsigned_abs().min(10) as usize;
                        work(async move { input::tap(vec![combo(key); n]).await })
                    }
                    DialEv::Down | DialEv::Touch { .. } => {
                        let (key, _) = media_key(idx)?;
                        work(async move { input::tap(vec![combo(key)]).await })
                    }
                    DialEv::Up => None,
                }
            }
            (BRIGHTNESS, DialEv::Rotate(t)) => {
                let cur = self.settings.devices.get(&dev).map(|s| s.brightness as i32).unwrap_or(60);
                self.set_brightness(&dev, (cur + t * 5).clamp(0, 100) as u8);
                None
            }
            (BRIGHTNESS, DialEv::Down) => {
                let cur = self.settings.devices.get(&dev).map(|s| s.brightness).unwrap_or(60);
                self.set_brightness(&dev, if cur > 50 { 10 } else { 100 });
                None
            }
            (MOUSE_WHEEL, DialEv::Rotate(t)) => {
                let invert = if a.setting_bool("invert", false) { -1 } else { 1 };
                let step = a.setting_i64("step", 1).clamp(1, 20) as i32;
                let horizontal = a.setting_str("direction") == "horizontal" || a.setting_bool("horizontal", false);
                // Clockwise scrolls down, like a mouse wheel turned towards you.
                let amount = -t * step * invert;
                let r = if horizontal { input::wheel(0, -amount) } else { input::wheel(amount, 0) };
                r.err().map(|e| -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send>> { Box::pin(async move { Err(e) }) })
            }
            (ACTION_WHEEL, DialEv::Rotate(t)) => {
                self.stack_step(ctx, t);
                None
            }
            (ACTION_WHEEL, DialEv::Down) => {
                let i = self.stack_index(ctx);
                self.run_children(ctx, vec![(0, i)])
            }
            (ACTION_TRIGGER, ev) => {
                let routine = match ev {
                    DialEv::Rotate(t) if t < 0 => 0,
                    DialEv::Rotate(_) => 1,
                    DialEv::Down => 2,
                    DialEv::Touch { .. } => 3,
                    DialEv::Up => return None,
                };
                self.run_routine(ctx, routine)
            }
            (DIAL_STACK, DialEv::Down) => {
                self.stack_step(ctx, 1);
                None
            }
            (DIAL_STACK, ev) => {
                // Everything else goes to the stack's current dial action.
                let i = self.stack_index(ctx);
                let child = a.actions.as_ref().and_then(|r| r.first()).and_then(|r| r.actions.get(i)).map(|c| c.action_id.clone())?;
                let builtin = self.instances.get(&child).map(|c| c.builtin).unwrap_or(false);
                if builtin {
                    return self.builtin_dial(&child, ev);
                }
                let cdata = self.ctx_action(&child).cloned()?;
                let plugin = self.instances.get(&child).map(|c| c.plugin.clone()).unwrap_or_default();
                let slot = self.instances.get(ctx).and_then(|i| i.slot);
                let base = json!({ "settings": cdata.settings, "coordinates": slot.map(|s| s.coordinates()), "controller": "Encoder" });
                let (event, payload) = match ev {
                    DialEv::Rotate(t) => {
                        let mut p = base;
                        p["ticks"] = json!(t);
                        p["pressed"] = json!(false);
                        ("dialRotate", p)
                    }
                    DialEv::Touch { x, y, long } => {
                        let mut p = base;
                        p["tapPos"] = json!([x, y]);
                        p["hold"] = json!(long);
                        ("touchTap", p)
                    }
                    DialEv::Up => ("dialUp", base),
                    DialEv::Down => unreachable!(),
                };
                self.send_plugin(&plugin, json!({ "event": event, "action": cdata.uuid, "context": child, "device": dev, "payload": payload }));
                None
            }
            (PLAY_AUDIO, DialEv::Down) | (STOP_AUDIO, DialEv::Down) | (TIMER, DialEv::Down) | (SWITCH_PROFILE, DialEv::Down) | (OPEN, DialEv::Down) | (WEBSITE, DialEv::Down) => self.builtin_down(ctx),
            (TIMER, DialEv::Up) => self.builtin_up(ctx, Duration::ZERO),
            (MIC_MUTE, DialEv::Down) => self.builtin_down(ctx),
            (MIC_MUTE, DialEv::Rotate(t)) => work(async move { system::step_mic_volume(t.clamp(-20, 20) * 2).await }),
            (PAGE_NEXT | PAGE_PREV | PAGE_INDICATOR, DialEv::Rotate(t)) => {
                self.page_step(&dev, t.signum());
                None
            }
            _ => None,
        }
    }

    pub fn builtin_tick(&mut self) {
        let now = now_ms();
        let timers: Vec<String> = self.instances.values().filter(|i| i.uuid == TIMER).map(|i| i.context.clone()).collect();
        for ctx in timers {
            let mut finished = false;
            if let Some(inst) = self.instances.get_mut(&ctx) {
                let running = inst.rt.get("running").and_then(|r| r.as_bool()).unwrap_or(false);
                let total = inst.rt.get("total").and_then(|r| r.as_i64()).unwrap_or(60).max(1);
                let mut remaining = inst.rt.get("remaining_ms").and_then(|r| r.as_i64()).unwrap_or(0);
                if running {
                    let last = inst.rt.get("last").and_then(|r| r.as_u64()).unwrap_or(now);
                    remaining -= now.saturating_sub(last) as i64;
                    inst.rt["last"] = json!(now);
                    if remaining <= 0 {
                        remaining = 0;
                        inst.rt["running"] = json!(false);
                        finished = true;
                    }
                    inst.rt["remaining_ms"] = json!(remaining);
                    inst.progress = Some(1.0 - remaining as f64 / (total * 1000) as f64);
                }
                inst.dyn_title = Some(fmt_duration(remaining));
            }
            if finished {
                self.show_overlay(&ctx, Overlay::Alert);
                let sound = self.ctx_action(&ctx).map(|a| a.setting_str("sound")).unwrap_or_default();
                let sound = if sound.is_empty() { "/usr/share/sounds/freedesktop/stereo/complete.oga".to_string() } else { sound };
                let id = format!("timer-{ctx}");
                tokio::spawn(async move {
                    if PathBuf::from(&sound).exists() {
                        audio::play(audio::PlayRequest { id, path: sound.into(), volume: 1.0, fade_in: Duration::ZERO, fade_out: Duration::ZERO, looping: false, restart: true }).await.ok();
                    }
                });
            }
        }
        let indicators: Vec<String> = self.instances.values().filter(|i| i.uuid == PAGE_INDICATOR).map(|i| i.context.clone()).collect();
        for c in indicators {
            self.update_page_indicator(&c);
        }
    }

    pub fn builtin_windows_changed(&mut self) {
        let apps: Vec<String> = self.instances.values().filter(|i| i.uuid == OPEN_APP).map(|i| i.context.clone()).collect();
        for c in apps {
            self.update_app_running(&c);
        }
    }

    fn update_app_running(&mut self, ctx: &str) {
        let Some(a) = self.ctx_action(ctx).cloned() else { return };
        let Some(app) = app_from_settings(&a.settings) else { return };
        let tx = self.tx.clone();
        let ctx = ctx.to_string();
        tokio::spawn(async move {
            let running = !system::app_windows(&app).await.is_empty();
            tx.send(Msg::Exec(Box::new(move |e: &mut Engine| {
                if let Some(i) = e.instances.get_mut(&ctx) {
                    if i.running != running {
                        i.running = running;
                        e.render_ctx(&ctx);
                    }
                }
            })))
            .ok();
        });
    }

    /// Shows the microphone's real mute state on every Mute Microphone action,
    /// whoever muted it.
    pub fn refresh_mic(&mut self) {
        if !self.instances.values().any(|i| i.uuid == MIC_MUTE) {
            return;
        }
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let Some(muted) = system::mic_muted().await else { return };
            tx.send(Msg::Exec(Box::new(move |e: &mut Engine| {
                let state = muted as u32;
                let stale: Vec<String> = e.instances.values().filter(|i| i.uuid == MIC_MUTE).map(|i| i.context.clone()).filter(|c| e.ctx_action(c).map(|a| a.state != state).unwrap_or(false)).collect();
                for c in stale {
                    e.set_state(&c, state);
                }
            })))
            .ok();
        });
    }

    fn update_page_indicator(&mut self, ctx: &str) {
        let dev = self.device_of(ctx);
        let Some(d) = self.devices.get(&dev) else { return };
        let pages = d.profile.and_then(|p| self.store.get(&p)).map(|p| p.top_pages()).unwrap_or_default();
        let cur = d.folders.first().copied().or(d.page).and_then(|pg| pages.iter().position(|x| *x == pg)).map(|i| i + 1).unwrap_or(1);
        let title = format!("{cur}/{}", pages.len().max(1));
        if let Some(i) = self.instances.get_mut(ctx) {
            i.dyn_title = Some(title);
        }
    }

    fn set_brightness(&mut self, dev: &str, value: u8) {
        self.settings.device(dev).brightness = value;
        self.settings.save();
        if let Some(d) = self.devices.get(dev) {
            d.tx.send(crate::device::RenderJob::Brightness(value)).ok();
        }
        self.emit_state();
    }

    /// Opens the Virtual Stream Deck window, or closes the one we opened.
    pub fn toggle_vsd_window(&mut self) {
        if let Some(pid) = self.vsd_pid.take() {
            if unsafe { libc::kill(pid as i32, 0) } == 0 {
                unsafe {
                    libc::kill(pid as i32, libc::SIGTERM);
                }
                return;
            }
        }
        let device = self.devices.values().find(|d| d.info.is_virtual).map(|d| d.info.id.clone()).unwrap_or_default();
        match std::process::Command::new(crate::plugins::gui_binary()).arg("vsd").arg(device).stdin(std::process::Stdio::null()).spawn() {
            Ok(mut child) => {
                self.vsd_pid = Some(child.id());
                // Reap the window when it closes so it does not linger as a zombie.
                std::thread::spawn(move || {
                    child.wait().ok();
                });
            }
            Err(e) => tracing::warn!("cannot open the Virtual Stream Deck window: {e}"),
        }
    }

    fn next_profile(&self, dev: &str) -> Option<Uuid> {
        let d = self.devices.get(dev)?;
        let list: Vec<Uuid> = self.store.for_device(dev, d.spec()).filter(|p| p.manifest.app_identifier.is_none()).map(|p| p.id).collect();
        if list.len() < 2 {
            return None;
        }
        let i = d.profile.and_then(|p| list.iter().position(|x| *x == p)).unwrap_or(0);
        Some(list[(i + 1) % list.len()])
    }

    fn open_folder(&mut self, ctx: &str, folder: &str) {
        let dev = self.device_of(ctx);
        let Some(d) = self.devices.get(&dev) else { return };
        let (Some(profile), Some(page)) = (d.profile, d.page) else { return };
        let spec = d.spec();
        let folder_id = match folder.parse::<Uuid>() {
            Ok(id) if self.store.get(&profile).map(|p| p.pages.contains_key(&id)).unwrap_or(false) => id,
            _ => {
                // A folder action without a folder (e.g. made by hand) gets one now.
                let Ok(id) = self.store.add_page(&profile, spec, true) else { return };
                if let Some(loc) = self.instances.get(ctx).and_then(|i| i.loc.clone()) {
                    if let Some(a) = self.action_at_mut(&loc) {
                        a.settings = json!({ "ProfileUUID": id.to_string() });
                    }
                }
                id
            }
        };
        if let Some(d) = self.devices.get_mut(&dev) {
            d.folders.push(page);
            d.page = Some(folder_id);
        }
        // Release the key on the folder's page before it changes underneath.
        if let Some(d) = self.devices.get_mut(&dev) {
            d.pressed.clear();
        }
        self.show(&dev);
        self.emit_state();
    }

    pub fn close_folder(&mut self, dev: &str) {
        let Some(d) = self.devices.get_mut(dev) else { return };
        let parent = d.folders.pop();
        let profile = d.profile;
        let current = d.page;
        let target = parent.or_else(|| {
            let p = self.store.get(&profile?)?;
            p.parent_of(&current?).or_else(|| p.current_page())
        });
        if let (Some(t), Some(d)) = (target, self.devices.get_mut(dev)) {
            d.page = Some(t);
            d.pressed.clear();
        }
        self.show(dev);
        self.emit_state();
    }

    fn soundboard_down(&mut self, ctx: &str, a: &crate::model::ActionData) -> Work {
        let path = a.setting_str("path");
        let mode = a.setting_i64("actionType", 0);
        let fade_type = a.setting_i64("fadeType", 0);
        let fade_len = Duration::from_millis(a.setting_i64("fadeLen", 0).max(0) as u64);
        let volume = (a.setting_f64("volume", 100.0) / 100.0).clamp(0.0, 1.5) as f32;
        let id = ctx.to_string();
        let tx = self.tx.clone();
        work(async move {
            if path.is_empty() {
                bail!("no audio file set");
            }
            let playing = audio::is_playing(&id).await;
            let fade_in = if fade_type == 1 || fade_type == 3 { fade_len } else { Duration::ZERO };
            let fade_out = if fade_type == 2 || fade_type == 3 { fade_len } else { Duration::ZERO };
            // Modes: 0 play/stop, 1 play/overlap, 2 play/restart, 3 loop/stop, 4 press & hold.
            if playing && (mode == 0 || mode == 3) {
                audio::stop(&id, fade_out);
                return Ok(());
            }
            let file = audio::pick_file(std::path::Path::new(&path)).ok_or_else(|| anyhow!("no audio at {path}"))?;
            audio::play(audio::PlayRequest { id: id.clone(), path: file, volume, fade_in, fade_out, looping: mode == 3, restart: mode != 1 }).await?;
            tx.send(Msg::Exec(Box::new(move |e: &mut Engine| e.render_ctx(&id)))).ok();
            Ok(())
        })
    }

    // -----------------------------------------------------------------------
    // Containers: multi actions, key logic, action wheel, dial stack
    // -----------------------------------------------------------------------

    pub fn run_routine(&mut self, ctx: &str, routine: usize) -> Work {
        let Some(a) = self.ctx_action(ctx) else { return None };
        let n = a.actions.as_ref().and_then(|r| r.get(routine)).map(|r| r.actions.len()).unwrap_or(0);
        if n == 0 {
            return None;
        }
        self.run_children(ctx, (0..n).map(|i| (routine, i)).collect())
    }

    /// Presses and releases each child in turn, waiting for one to finish
    /// before the next (so a Delay really delays the rest).
    pub fn run_children(&mut self, parent: &str, which: Vec<(usize, usize)>) -> Work {
        let Some(a) = self.ctx_action(parent) else { return None };
        let mut steps = Vec::new();
        for (r, i) in which {
            if let Some(child) = a.actions.as_ref().and_then(|rs| rs.get(r)).and_then(|rt| rt.actions.get(i)) {
                // Multi Action children can ask for a specific state ("userDesiredState").
                let desired = child.extra.get("UserDesiredState").or_else(|| child.settings.get("userDesiredState")).and_then(|v| v.as_u64()).map(|v| v as u32);
                steps.push((child.action_id.clone(), desired));
            }
        }
        let tx = self.tx.clone();
        let parent = parent.to_string();
        if let Some(inst) = self.instances.get_mut(&parent) {
            inst.progress = Some(0.0);
        }
        work(async move {
            let total = steps.len();
            for (n, (ctx, desired)) in steps.into_iter().enumerate() {
                let (otx, orx) = oneshot::channel::<Work>();
                let c = ctx.clone();
                tx.send(Msg::Exec(Box::new(move |e: &mut Engine| {
                    otx.send(e.trigger_down(&c, desired)).ok();
                })))
                .ok();
                if let Ok(Some(w)) = orx.await {
                    if let Err(err) = w.await {
                        let msg = format!("{err:#}");
                        let p = parent.clone();
                        tx.send(Msg::Exec(Box::new(move |e: &mut Engine| e.alert(&p, &msg)))).ok();
                    }
                }
                tokio::time::sleep(Duration::from_millis(30)).await;
                let (otx, orx) = oneshot::channel::<Work>();
                let c = ctx.clone();
                tx.send(Msg::Exec(Box::new(move |e: &mut Engine| {
                    otx.send(e.trigger_up(&c, Duration::ZERO, desired)).ok();
                })))
                .ok();
                if let Ok(Some(w)) = orx.await {
                    w.await.ok();
                }
                let p = parent.clone();
                let frac = (n + 1) as f64 / total as f64;
                tx.send(Msg::Exec(Box::new(move |e: &mut Engine| {
                    if let Some(i) = e.instances.get_mut(&p) {
                        i.progress = if frac >= 1.0 { None } else { Some(frac) };
                    }
                    e.render_ctx(&p);
                })))
                .ok();
            }
            Ok(())
        })
    }

    fn stack_index(&self, ctx: &str) -> usize {
        self.instances.get(ctx).and_then(|i| i.rt.get("index")).and_then(|v| v.as_u64()).unwrap_or(0) as usize
    }

    fn stack_step(&mut self, ctx: &str, delta: i32) {
        let n = self.ctx_action(ctx).and_then(|a| a.actions.as_ref()).and_then(|r| r.first()).map(|r| r.actions.len()).unwrap_or(0);
        if n == 0 {
            return;
        }
        let i = (self.stack_index(ctx) as i32 + delta).rem_euclid(n as i32);
        if let Some(inst) = self.instances.get_mut(ctx) {
            inst.rt["index"] = json!(i);
        }
        self.update_stack_display(ctx);
        self.render_ctx(ctx);
    }

    /// Action Wheel and Dial Stack show their current child on the strip.
    fn update_stack_display(&mut self, ctx: &str) {
        let i = self.stack_index(ctx);
        let Some(a) = self.ctx_action(ctx).cloned() else { return };
        let Some(child) = a.actions.as_ref().and_then(|r| r.first()).and_then(|r| r.actions.get(i)).cloned() else { return };
        let n = a.actions.as_ref().and_then(|r| r.first()).map(|r| r.actions.len()).unwrap_or(0);
        let child_visual = self.visual_for(&child.action_id);
        let def_name = self.registry.action(&child.uuid).map(|d| d.name.clone()).unwrap_or_else(|| child.name.clone());
        let title = child_visual.title.as_ref().map(|t| t.text.clone()).filter(|t| !t.is_empty()).unwrap_or(def_name);
        if let Some(inst) = self.instances.get_mut(ctx) {
            inst.dyn_title = Some(format!("{title} ({}/{n})", i + 1));
            inst.dyn_image = child_visual.image.clone();
            if let Some(ImageSource::File(p)) = &child_visual.image {
                inst.feedback.insert("icon".into(), json!({ "value": p.to_string_lossy() }));
            }
            inst.feedback.insert("title".into(), json!({ "value": inst.dyn_title.clone().unwrap_or_default() }));
        }
    }
}

pub fn app_from_settings(settings: &Value) -> Option<system::DesktopApp> {
    for key in ["bundle_path", "app_path", "desktop", "app_name", "path"] {
        if let Some(v) = settings.get(key).and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
            if let Some(app) = system::find_app(v.trim_matches('"')) {
                return Some(app);
            }
        }
    }
    None
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn fmt_duration(ms: i64) -> String {
    let s = (ms.max(0) + 999) / 1000;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

#[allow(dead_code)]
fn _instant() -> Instant {
    Instant::now()
}
