//! Physical and virtual decks. Each device gets a render worker that takes key
//! and screen descriptions, coalesces bursts (a page switch redraws every key
//! at once), rasterises them and writes them to the hardware. The same pixels
//! are published as previews for the editor, the Virtual Stream Deck and the
//! Omarchy bar.

use crate::catalog::{self, DeviceSpec};
use crate::render::{self, KeyVisual};
use elgato_streamdeck::asynchronous::AsyncStreamDeck;
use elgato_streamdeck::info::Kind;
use elgato_streamdeck::DeviceStateUpdate;
use image::{DynamicImage, RgbaImage};
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};

#[derive(Clone, Debug)]
pub enum DeviceInput {
    KeyDown(u8),
    KeyUp(u8),
    DialDown(u8),
    DialUp(u8),
    DialRotate(u8, i32),
    /// Touch strip tap in strip coordinates.
    Touch { x: u16, y: u16, long: bool },
    Swipe { from: (u16, u16), to: (u16, u16) },
    TouchPointDown(u8),
    TouchPointUp(u8),
}

#[derive(Clone, Debug)]
pub enum RenderJob {
    Key(u8, KeyVisual),
    /// A complete SVG for the touch strip / info bar.
    Screen(String),
    Brightness(u8),
    /// Neo touch point LED colour.
    TouchPoint(u8, [u8; 3]),
    /// Redraw everything from the last known state (after a reconnect or wake).
    Refresh,
}

#[derive(Clone, Debug)]
pub struct Preview {
    pub device: String,
    /// `key` or `screen`.
    pub target: &'static str,
    pub index: u8,
    pub png: Arc<Vec<u8>>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub spec: &'static DeviceSpec,
    pub serial: String,
    pub firmware: String,
    #[serde(rename = "virtual")]
    pub is_virtual: bool,
}

pub enum EngineEvent {
    Connected(DeviceInfo, mpsc::UnboundedSender<RenderJob>),
    Disconnected(String),
    Input(String, DeviceInput),
}

/// Retains the latest frame of every key so previews can be replayed to
/// late subscribers, and does the actual drawing.
struct Worker {
    id: String,
    spec: &'static DeviceSpec,
    key_px: u32,
    hw: Option<AsyncStreamDeck>,
    previews: broadcast::Sender<Preview>,
    last_keys: BTreeMap<u8, KeyVisual>,
    last_screen: Option<String>,
    brightness: u8,
}

impl Worker {
    async fn run(mut self, mut rx: mpsc::UnboundedReceiver<RenderJob>, frames: Arc<std::sync::Mutex<BTreeMap<(String, &'static str, u8), Arc<Vec<u8>>>>>) {
        while let Some(first) = rx.recv().await {
            // Coalesce: keep only the newest job per key.
            let mut keys: BTreeMap<u8, KeyVisual> = BTreeMap::new();
            let mut screen: Option<String> = None;
            let mut others = Vec::new();
            let handle = |job: RenderJob, keys: &mut BTreeMap<u8, KeyVisual>, screen: &mut Option<String>, others: &mut Vec<RenderJob>| match job {
                RenderJob::Key(i, v) => {
                    keys.insert(i, v);
                }
                RenderJob::Screen(s) => *screen = Some(s),
                other => others.push(other),
            };
            handle(first, &mut keys, &mut screen, &mut others);
            while let Ok(job) = rx.try_recv() {
                handle(job, &mut keys, &mut screen, &mut others);
            }
            for job in others {
                match job {
                    RenderJob::Brightness(b) => {
                        self.brightness = b;
                        if let Some(hw) = &self.hw {
                            if let Err(e) = hw.set_brightness(b).await {
                                tracing::warn!("{}: brightness: {e}", self.id);
                            }
                        }
                    }
                    RenderJob::TouchPoint(i, c) => {
                        if let Some(hw) = &self.hw {
                            hw.set_touchpoint_color(i, c[0], c[1], c[2]).await.ok();
                        }
                    }
                    RenderJob::Refresh => {
                        for (k, v) in self.last_keys.clone() {
                            keys.entry(k).or_insert(v);
                        }
                        if screen.is_none() {
                            screen = self.last_screen.clone();
                        }
                        if let Some(hw) = &self.hw {
                            hw.set_brightness(self.brightness).await.ok();
                        }
                    }
                    _ => {}
                }
            }
            let px = self.key_px;
            let lcd = self.spec.lcd;
            let changed: Vec<(u8, KeyVisual)> = keys.into_iter().collect();
            let rendered = tokio::task::spawn_blocking(move || {
                let keys: Vec<(u8, RgbaImage)> = changed.iter().map(|(i, v)| (*i, render::render_key(v, px))).collect();
                let screen = screen.map(|svg| {
                    let (w, h) = lcd.unwrap_or((200, 100));
                    (svg.clone(), render::rasterize(&svg, w, h))
                });
                (changed, keys, screen)
            })
            .await;
            let Ok((visuals, images, screen)) = rendered else { continue };
            for (i, v) in visuals {
                self.last_keys.insert(i, v);
            }
            for (i, img) in &images {
                if let Some(hw) = &self.hw {
                    if let Err(e) = hw.set_button_image(*i, DynamicImage::ImageRgba8(img.clone())).await {
                        tracing::debug!("{}: key {i}: {e}", self.id);
                    }
                }
                let png = Arc::new(render::png_bytes(img));
                frames.lock().unwrap().insert((self.id.clone(), "key", *i), png.clone());
                self.previews.send(Preview { device: self.id.clone(), target: "key", index: *i, png }).ok();
            }
            if let Some((svg, img)) = screen {
                self.last_screen = Some(svg);
                if let Some(hw) = &self.hw {
                    let res = match hw.kind() {
                        Kind::Plus | Kind::PlusXl | Kind::Neo => match hw.kind().lcd_image_format() {
                            Some(fmt) => match elgato_streamdeck::images::convert_image_with_format_async(fmt, DynamicImage::ImageRgba8(img.clone())) {
                                Ok(data) => hw.write_lcd_fill(&data).await,
                                Err(e) => Err(e),
                            },
                            None => Ok(()),
                        },
                        _ => Ok(()),
                    };
                    if let Err(e) = res {
                        tracing::debug!("{}: screen: {e}", self.id);
                    }
                }
                let png = Arc::new(render::png_bytes(&img));
                frames.lock().unwrap().insert((self.id.clone(), "screen", 0), png.clone());
                self.previews.send(Preview { device: self.id.clone(), target: "screen", index: 0, png }).ok();
            }
            if !images.is_empty() {
                if let Some(hw) = &self.hw {
                    hw.flush().await.ok();
                }
            }
        }
    }
}

pub type FrameStore = Arc<std::sync::Mutex<BTreeMap<(String, &'static str, u8), Arc<Vec<u8>>>>>;

pub struct DeviceManager {
    pub engine: mpsc::UnboundedSender<EngineEvent>,
    pub previews: broadcast::Sender<Preview>,
    pub frames: FrameStore,
}

impl DeviceManager {
    pub fn spawn_virtual(&self, id: &str, name: &str, spec: &'static DeviceSpec) {
        let (tx, rx) = mpsc::unbounded_channel();
        let worker = Worker { id: id.into(), spec, key_px: spec.key_px.max(72) * 2, hw: None, previews: self.previews.clone(), last_keys: BTreeMap::new(), last_screen: None, brightness: 100 };
        tokio::spawn(worker.run(rx, self.frames.clone()));
        let info = DeviceInfo { id: id.into(), name: name.into(), spec, serial: id.into(), firmware: String::new(), is_virtual: true };
        self.engine.send(EngineEvent::Connected(info, tx)).ok();
    }

    /// Polls for decks every two seconds. hidapi has no hotplug callback on
    /// Linux, and a poll this slow costs nothing measurable.
    pub fn spawn_hardware_watch(&self) {
        let engine = self.engine.clone();
        let previews = self.previews.clone();
        let frames = self.frames.clone();
        tokio::spawn(async move {
            let mut hid = match elgato_streamdeck::new_hidapi() {
                Ok(h) => h,
                Err(e) => {
                    tracing::error!("hidapi unavailable: {e}");
                    return;
                }
            };
            let connected: Arc<std::sync::Mutex<HashSet<String>>> = Arc::default();
            let mut warned: HashSet<String> = HashSet::new();
            loop {
                if let Err(e) = elgato_streamdeck::refresh_device_list(&mut hid) {
                    tracing::warn!("hid refresh: {e}");
                }
                for (kind, serial) in elgato_streamdeck::list_devices(&hid) {
                    let id = if serial.is_empty() { format!("{kind:?}") } else { serial.clone() };
                    if connected.lock().unwrap().contains(&id) {
                        continue;
                    }
                    let deck = match AsyncStreamDeck::connect(&hid, kind, &serial) {
                        Ok(d) => d,
                        Err(e) => {
                            if warned.insert(id.clone()) {
                                tracing::warn!(
                                    "found {kind:?} {serial} but could not open it ({e}). Another program may own it, or the udev rule granting access is missing."
                                );
                            }
                            continue;
                        }
                    };
                    warned.remove(&id);
                    connected.lock().unwrap().insert(id.clone());
                    let spec = catalog::spec_for_kind(kind);
                    let firmware = deck.firmware_version().await.unwrap_or_default();
                    let name = deck.product().await.ok().filter(|s| !s.is_empty()).unwrap_or_else(|| spec.name.to_string());
                    deck.reset().await.ok();
                    let (tx, rx) = mpsc::unbounded_channel();
                    let key_px = kind.key_image_format().size.0 as u32;
                    let worker = Worker { id: id.clone(), spec, key_px: key_px.max(1), hw: Some(deck.clone()), previews: previews.clone(), last_keys: BTreeMap::new(), last_screen: None, brightness: 60 };
                    tokio::spawn(worker.run(rx, frames.clone()));
                    let info = DeviceInfo { id: id.clone(), name, spec, serial: serial.clone(), firmware, is_virtual: false };
                    tracing::info!("connected {} ({:?}, {})", info.name, kind, id);
                    engine.send(EngineEvent::Connected(info, tx)).ok();
                    let reader = deck.get_reader();
                    let engine2 = engine.clone();
                    let connected2 = connected.clone();
                    tokio::spawn(async move {
                        loop {
                            match reader.read(60.0).await {
                                Ok(updates) => {
                                    for u in updates {
                                        let input = match u {
                                            DeviceStateUpdate::ButtonDown(k) => DeviceInput::KeyDown(k),
                                            DeviceStateUpdate::ButtonUp(k) => DeviceInput::KeyUp(k),
                                            DeviceStateUpdate::EncoderDown(e) => DeviceInput::DialDown(e),
                                            DeviceStateUpdate::EncoderUp(e) => DeviceInput::DialUp(e),
                                            DeviceStateUpdate::EncoderTwist(e, t) => DeviceInput::DialRotate(e, t as i32),
                                            DeviceStateUpdate::TouchPointDown(p) => DeviceInput::TouchPointDown(p),
                                            DeviceStateUpdate::TouchPointUp(p) => DeviceInput::TouchPointUp(p),
                                            DeviceStateUpdate::TouchScreenPress(x, y) => DeviceInput::Touch { x, y, long: false },
                                            DeviceStateUpdate::TouchScreenLongPress(x, y) => DeviceInput::Touch { x, y, long: true },
                                            DeviceStateUpdate::TouchScreenSwipe(a, b) => DeviceInput::Swipe { from: a, to: b },
                                        };
                                        engine2.send(EngineEvent::Input(id.clone(), input)).ok();
                                    }
                                }
                                Err(e) => {
                                    tracing::info!("{id} disconnected: {e}");
                                    connected2.lock().unwrap().remove(&id);
                                    engine2.send(EngineEvent::Disconnected(id.clone())).ok();
                                    break;
                                }
                            }
                        }
                    });
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        });
    }
}
