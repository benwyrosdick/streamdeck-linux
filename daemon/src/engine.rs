//! The engine owns all state: devices, the profile store, plugin connections
//! and the live action instances on screen. It runs as a single task fed by
//! one channel, so nothing in here needs a lock.

use crate::catalog::{self, DeviceSpec};
use crate::device::{DeviceInfo, DeviceInput, EngineEvent, RenderJob};
use crate::model::{page_dir_name, ActionData};
use crate::plugins::{self, Registry, RunningPlugin};
use crate::render::{self, ImageSource, KeyVisual, Overlay, Title};
use crate::sdk::{ConnKind, SdkEvent};
use crate::settings::{self, Settings};
use crate::store::ProfileStore;
use crate::system::HyprEvent;
use anyhow::{anyhow, Result};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use uuid::Uuid;

pub type Work = Option<Pin<Box<dyn Future<Output = Result<()>> + Send>>>;

pub fn work<F: Future<Output = Result<()>> + Send + 'static>(f: F) -> Work {
    Some(Box::pin(f))
}

pub enum Msg {
    Device(EngineEvent),
    Sdk(SdkEvent),
    Control { id: Value, method: String, params: Value, reply: mpsc::UnboundedSender<String> },
    Subscribe { client: u64, tx: mpsc::UnboundedSender<String> },
    Unsubscribe(u64),
    Hypr(HyprEvent),
    Sleep(bool),
    Tick,
    Hold { device: String, slot: Slot, seq: u64 },
    Exec(Box<dyn FnOnce(&mut Engine) + Send>),
    PluginExited(String),
    Shutdown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Ctl {
    Keypad,
    Encoder,
    Neo,
}

impl Ctl {
    pub fn name(&self) -> &'static str {
        match self {
            Ctl::Keypad => "Keypad",
            Ctl::Encoder => "Encoder",
            Ctl::Neo => "Neo",
        }
    }

    pub fn parse(s: &str) -> Option<Ctl> {
        match s {
            "Keypad" => Some(Ctl::Keypad),
            "Encoder" => Some(Ctl::Encoder),
            "Neo" => Some(Ctl::Neo),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Slot {
    pub ctl: Ctl,
    pub col: u8,
    pub row: u8,
}

impl Slot {
    pub fn key(col: u8, row: u8) -> Slot {
        Slot { ctl: Ctl::Keypad, col, row }
    }
    pub fn dial(i: u8) -> Slot {
        Slot { ctl: Ctl::Encoder, col: i, row: 0 }
    }
    pub fn pos(&self) -> String {
        format!("{},{}", self.col, self.row)
    }
    pub fn parse(ctl: &str, pos: &str) -> Option<Slot> {
        let (c, r) = pos.split_once(',')?;
        Some(Slot { ctl: Ctl::parse(ctl)?, col: c.trim().parse().ok()?, row: r.trim().parse().ok()? })
    }
    pub fn coordinates(&self) -> Value {
        json!({ "column": self.col, "row": self.row })
    }
}

/// Where an action lives in the profile store. `path` descends into child
/// actions: each step is (routine index, action index).
#[derive(Clone, Debug, PartialEq)]
pub struct Loc {
    pub profile: Uuid,
    pub page: Uuid,
    pub ctl: Ctl,
    pub pos: String,
    pub path: Vec<(usize, usize)>,
}

#[derive(Default, Debug)]
pub struct Instance {
    pub context: String,
    pub device: String,
    pub loc: Option<Loc>,
    pub slot: Option<Slot>,
    pub uuid: String,
    pub plugin: String,
    pub builtin: bool,
    pub parent: Option<String>,
    pub title: HashMap<u32, String>,
    pub image: HashMap<u32, ImageSource>,
    pub overlay: Option<(Overlay, Instant)>,
    pub feedback: Map<String, Value>,
    pub layout: Option<Value>,
    pub trigger: Option<Value>,
    pub progress: Option<f64>,
    pub running: bool,
    /// Title computed by a built-in action (clock, timer, page number).
    pub dyn_title: Option<String>,
    pub dyn_image: Option<ImageSource>,
    pub rt: Value,
}

pub struct Dev {
    pub info: DeviceInfo,
    pub tx: mpsc::UnboundedSender<RenderJob>,
    pub profile: Option<Uuid>,
    pub page: Option<Uuid>,
    pub folders: Vec<Uuid>,
    pub visible: BTreeMap<Slot, String>,
    pub asleep: bool,
    pub last_input: Instant,
    pub pressed: HashMap<Slot, (Instant, u64)>,
    pub seq: u64,
    pub last_visual: HashMap<u8, KeyVisual>,
    pub last_screen: Option<String>,
    /// Profile to return to when a smart (app-bound) profile loses focus.
    pub smart_return: Option<Uuid>,
    pub connected: bool,
}

impl Dev {
    pub fn spec(&self) -> &'static DeviceSpec {
        self.info.spec
    }
}

pub struct Engine {
    pub tx: mpsc::UnboundedSender<Msg>,
    pub store: ProfileStore,
    pub registry: Registry,
    pub settings: Settings,
    pub devices: BTreeMap<String, Dev>,
    pub instances: HashMap<String, Instance>,
    pub sdk_port: u16,
    plugin_conns: HashMap<String, (u64, mpsc::UnboundedSender<String>)>,
    pi_conns: HashMap<String, (u64, mpsc::UnboundedSender<String>)>,
    conn_index: HashMap<u64, (ConnKind, String)>,
    running: HashMap<String, RunningPlugin>,
    stopped_plugins: HashSet<String>,
    subscribers: HashMap<u64, mpsc::UnboundedSender<String>>,
    dirty_pages: HashSet<(Uuid, Uuid)>,
    dirty_manifests: HashSet<Uuid>,
    pub active_class: String,
    monitored_running: HashSet<String>,
    global_settings: HashMap<String, Value>,
    last_backup: Option<Instant>,
    /// The Virtual Stream Deck window the Toggle action opened, if any.
    pub vsd_pid: Option<u32>,
}

pub fn info_payload(e: &Engine, plugin: &str) -> Value {
    let devices: Vec<Value> = e.devices.values().filter(|d| d.connected).map(|d| device_info_json(d)).collect();
    let version = e.registry.plugins.get(plugin).map(|p| p.version.clone()).unwrap_or_default();
    json!({
        "application": { "font": ".AppleSystemUIFont", "language": e.settings.language, "platform": "linux", "platformVersion": os_version(), "version": "7.6.1.23033" },
        "plugin": { "uuid": plugin, "version": version },
        "devicePixelRatio": 2,
        "colors": {
            "buttonPressedBackgroundColor": "#303030FF",
            "buttonPressedBorderColor": "#646464FF",
            "buttonPressedTextColor": "#969696FF",
            "disabledColor": "#007AFF59",
            "highlightColor": "#007AFFFF",
            "mouseDownColor": "#2EA8FFFF"
        },
        "devices": devices,
    })
}

fn os_version() -> String {
    std::fs::read_to_string("/proc/sys/kernel/osrelease").map(|s| s.trim().to_string()).unwrap_or_default()
}

pub fn device_info_json(d: &Dev) -> Value {
    let s = d.spec();
    json!({ "id": d.info.id, "name": d.info.name, "size": { "columns": s.columns, "rows": s.rows }, "type": if d.info.is_virtual { 11 } else { s.sdk_type } })
}

impl Engine {
    pub fn new(tx: mpsc::UnboundedSender<Msg>, sdk_port: u16) -> Result<Engine> {
        let settings = Settings::load();
        let store = ProfileStore::open(crate::paths::profiles_dir())?;
        let registry = Registry::load(&settings.language);
        tracing::info!("{} profiles, {} plugins", store.profiles.len(), registry.plugins.len());
        Ok(Engine {
            tx,
            store,
            registry,
            settings,
            devices: BTreeMap::new(),
            instances: HashMap::new(),
            sdk_port,
            plugin_conns: HashMap::new(),
            pi_conns: HashMap::new(),
            conn_index: HashMap::new(),
            running: HashMap::new(),
            stopped_plugins: HashSet::new(),
            subscribers: HashMap::new(),
            dirty_pages: HashSet::new(),
            dirty_manifests: HashSet::new(),
            active_class: String::new(),
            monitored_running: HashSet::new(),
            global_settings: HashMap::new(),
            last_backup: None,
            vsd_pid: None,
        })
    }

    pub async fn run(mut self, mut rx: mpsc::UnboundedReceiver<Msg>) {
        self.start_all_plugins();
        while let Some(msg) = rx.recv().await {
            match msg {
                Msg::Device(e) => self.on_device_event(e),
                Msg::Sdk(e) => self.on_sdk(e),
                Msg::Control { id, method, params, reply } => {
                    let res = self.handle_control(&method, params);
                    let text = match res {
                        Ok(v) => json!({ "id": id, "result": v }).to_string(),
                        Err(e) => json!({ "id": id, "error": format!("{e:#}") }).to_string(),
                    };
                    reply.send(text).ok();
                }
                Msg::Subscribe { client, tx } => {
                    self.subscribers.insert(client, tx);
                }
                Msg::Unsubscribe(client) => {
                    self.subscribers.remove(&client);
                }
                Msg::Hypr(e) => self.on_hypr(e),
                Msg::Sleep(going) => {
                    if !going {
                        self.broadcast_plugins(json!({ "event": "systemDidWakeUp" }));
                        for d in self.devices.values() {
                            d.tx.send(RenderJob::Refresh).ok();
                        }
                    }
                }
                Msg::Tick => self.tick(),
                Msg::Hold { device, slot, seq } => self.on_hold(&device, slot, seq),
                Msg::Exec(f) => f(&mut self),
                Msg::PluginExited(uuid) => self.on_plugin_exit(&uuid),
                Msg::Shutdown => {
                    self.flush();
                    for d in self.devices.values() {
                        d.tx.send(RenderJob::Brightness(0)).ok();
                    }
                    for (_, mut r) in self.running.drain() {
                        r.child.start_kill().ok();
                    }
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    break;
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Events to subscribers (editor, bar)
    // -----------------------------------------------------------------------

    pub fn emit(&mut self, event: &str, data: Value) {
        let text = json!({ "event": event, "data": data }).to_string();
        self.subscribers.retain(|_, tx| tx.send(text.clone()).is_ok());
    }

    pub fn emit_state(&mut self) {
        let s = self.state_json();
        self.emit("state", s);
    }

    pub fn state_json(&self) -> Value {
        let devices: Vec<Value> = self
            .devices
            .values()
            .filter(|d| d.connected)
            .map(|d| {
                let profile = d.profile.and_then(|p| self.store.get(&p));
                let pages = profile.map(|p| p.top_pages()).unwrap_or_default();
                let page_index = d.page.and_then(|pg| pages.iter().position(|x| *x == pg));
                json!({
                    "id": d.info.id,
                    "name": self.settings.devices.get(&d.info.id).and_then(|s| s.name.clone()).unwrap_or_else(|| d.info.name.clone()),
                    "model": d.spec().id,
                    "modelName": d.spec().name,
                    "virtual": d.info.is_virtual,
                    "serial": d.info.serial,
                    "firmware": d.info.firmware,
                    "columns": d.spec().columns,
                    "rows": d.spec().rows,
                    "encoders": d.spec().encoders,
                    "touchPoints": d.spec().touch_points,
                    "screen": d.spec().lcd.map(|(w, h)| json!([w, h])),
                    "profile": d.profile.map(|p| p.to_string()),
                    "profileName": profile.map(|p| p.manifest.name.clone()),
                    "page": d.page.map(|p| p.to_string()),
                    "pageIndex": page_index,
                    "pageCount": pages.len(),
                    "inFolder": !d.folders.is_empty(),
                    "asleep": d.asleep,
                    "brightness": self.settings.devices.get(&d.info.id).map(|s| s.brightness).unwrap_or(60),
                    "sleepMinutes": self.settings.devices.get(&d.info.id).map(|s| s.sleep_minutes).unwrap_or(0),
                    "smartProfile": d.smart_return.is_some(),
                })
            })
            .collect();
        json!({ "devices": devices, "sdkPort": self.sdk_port, "activeApp": self.active_class })
    }

    // -----------------------------------------------------------------------
    // Devices
    // -----------------------------------------------------------------------

    fn on_device_event(&mut self, e: EngineEvent) {
        match e {
            EngineEvent::Connected(info, tx) => {
                let id = info.id.clone();
                let ds = self.settings.device(&id).clone();
                tx.send(RenderJob::Brightness(ds.brightness)).ok();
                let dev = Dev {
                    info,
                    tx,
                    profile: None,
                    page: None,
                    folders: Vec::new(),
                    visible: BTreeMap::new(),
                    asleep: false,
                    last_input: Instant::now(),
                    pressed: HashMap::new(),
                    seq: 0,
                    last_visual: HashMap::new(),
                    last_screen: None,
                    smart_return: None,
                    connected: true,
                };
                self.devices.insert(id.clone(), dev);
                let profile = self.pick_profile(&id, ds.profile.as_deref());
                self.switch_profile(&id, profile, None);
                let info = device_info_json(&self.devices[&id]);
                self.broadcast_plugins(json!({ "event": "deviceDidConnect", "device": id, "deviceInfo": info }));
                self.emit_state();
            }
            EngineEvent::Disconnected(id) => {
                let ctxs: Vec<String> = self.devices.get(&id).map(|d| d.visible.values().cloned().collect()).unwrap_or_default();
                for c in ctxs {
                    self.disappear(&c);
                }
                self.devices.remove(&id);
                self.broadcast_plugins(json!({ "event": "deviceDidDisconnect", "device": id }));
                self.emit_state();
            }
            EngineEvent::Input(id, input) => self.on_input(&id, input),
        }
    }

    /// Chooses which profile a device shows: the remembered one, else one
    /// bound to this device, else any profile with the same layout, else a
    /// fresh one.
    fn pick_profile(&mut self, dev: &str, remembered: Option<&str>) -> Uuid {
        let spec = self.devices[dev].spec();
        if let Some(p) = remembered.and_then(|r| r.parse::<Uuid>().ok()) {
            if self.store.get(&p).is_some() {
                return p;
            }
        }
        let mut candidates: Vec<(Uuid, bool)> = self.store.for_device(dev, spec).map(|p| (p.id, p.manifest.device.uuid == dev)).collect();
        candidates.sort_by_key(|(_, own)| !*own);
        if let Some((id, _)) = candidates.iter().find(|(id, _)| self.store.get(id).map(|p| p.manifest.app_identifier.is_none()).unwrap_or(false)) {
            let id = *id;
            self.bind_profile(id, dev);
            return id;
        }
        let id = self.store.create("Default Profile", spec, dev).expect("create profile");
        if self.devices[dev].info.is_virtual {
            self.seed_virtual_profile(id);
        }
        id
    }

    fn bind_profile(&mut self, profile: Uuid, dev: &str) {
        if let Some(p) = self.store.get_mut(&profile) {
            if p.manifest.device.uuid.is_empty() {
                p.manifest.device.uuid = dev.to_string();
                self.dirty_manifests.insert(profile);
            }
        }
    }

    /// The first time a virtual deck appears it gets a few useful keys so it
    /// is not an empty grid.
    fn seed_virtual_profile(&mut self, id: Uuid) {
        let Some(p) = self.store.get_mut(&id) else { return };
        let Some(page) = p.top_pages().first().copied() else { return };
        if let Some(data) = p.pages.get_mut(&page) {
            let k = data.controller_mut("Keypad");
            k.actions.insert("0,0".into(), crate::store::new_action("com.elgato.streamdeck.system.multimedia", "Multimedia", json!({"actionIdx": 1})));
            k.actions.insert("1,0".into(), crate::store::new_action("com.elgato.streamdeck.system.multimedia", "Multimedia", json!({"actionIdx": 4})));
            k.actions.insert("2,0".into(), crate::store::new_action("com.elgato.streamdeck.system.website", "Website", json!({"path": "https://omarchy.org", "openInBrowser": true})));
        }
        p.save_page(&page).ok();
    }

    pub fn switch_profile(&mut self, dev: &str, profile: Uuid, page: Option<Uuid>) {
        let Some(p) = self.store.get(&profile) else { return };
        let page = page.or_else(|| p.current_page());
        let Some(d) = self.devices.get_mut(dev) else { return };
        d.profile = Some(profile);
        d.page = page;
        d.folders.clear();
        self.show(dev);
        self.emit_state();
    }

    pub fn switch_page(&mut self, dev: &str, page: Uuid, remember: bool) {
        let Some(d) = self.devices.get_mut(dev) else { return };
        let Some(profile) = d.profile else { return };
        d.page = Some(page);
        if remember {
            d.folders.clear();
            if let Some(p) = self.store.get_mut(&profile) {
                if !p.is_folder(&page) {
                    p.manifest.pages.current = page.to_string();
                    self.dirty_manifests.insert(profile);
                }
            }
        }
        self.show(dev);
        self.emit_state();
    }

    pub fn page_step(&mut self, dev: &str, delta: i32) {
        let Some(d) = self.devices.get(dev) else { return };
        let Some(p) = d.profile.and_then(|p| self.store.get(&p)) else { return };
        let pages = p.top_pages();
        if pages.len() < 2 {
            return;
        }
        // From inside a folder, page navigation works on the root page.
        let current = d.folders.first().copied().or(d.page).and_then(|pg| pages.iter().position(|x| *x == pg)).unwrap_or(0) as i32;
        let next = (current + delta).rem_euclid(pages.len() as i32) as usize;
        self.switch_page(dev, pages[next], true);
    }

    /// Diffs what should be on screen against what is, sending willAppear /
    /// willDisappear for the difference, then redraws.
    pub fn show(&mut self, dev_id: &str) {
        let Some(d) = self.devices.get(dev_id) else { return };
        let (Some(profile), Some(page)) = (d.profile, d.page) else { return };
        let spec = d.spec();
        let mut desired: BTreeMap<Slot, (String, Loc)> = BTreeMap::new();
        if let Some(pd) = self.store.get(&profile).and_then(|p| p.pages.get(&page)) {
            for c in &pd.controllers {
                let Some(ctl) = Ctl::parse(&c.kind) else { continue };
                for (pos, a) in &c.actions {
                    let Some(slot) = Slot::parse(&c.kind, pos) else { continue };
                    let fits = match ctl {
                        Ctl::Keypad => slot.col < spec.columns && slot.row < spec.rows,
                        Ctl::Encoder => slot.col < spec.encoders,
                        Ctl::Neo => true,
                    };
                    if fits {
                        desired.insert(slot, (a.action_id.clone(), Loc { profile, page, ctl, pos: pos.clone(), path: vec![] }));
                    }
                }
            }
        }
        let current: Vec<(Slot, String)> = d.visible.iter().map(|(s, c)| (*s, c.clone())).collect();
        for (slot, ctx) in current {
            let keep = desired.get(&slot).map(|(c, _)| *c == ctx).unwrap_or(false);
            if !keep {
                self.disappear(&ctx);
                if let Some(d) = self.devices.get_mut(dev_id) {
                    d.visible.remove(&slot);
                }
            }
        }
        for (slot, (ctx, loc)) in desired {
            let present = self.devices.get(dev_id).map(|d| d.visible.get(&slot) == Some(&ctx)).unwrap_or(false);
            if !present {
                self.appear(dev_id, slot, ctx.clone(), loc);
                if let Some(d) = self.devices.get_mut(dev_id) {
                    d.visible.insert(slot, ctx);
                }
            }
        }
        self.render_device(dev_id);
    }

    pub fn action_at(&self, loc: &Loc) -> Option<&ActionData> {
        let p = self.store.get(&loc.profile)?;
        let page = p.pages.get(&loc.page)?;
        let mut a = page.controller(loc.ctl.name())?.actions.get(&loc.pos)?;
        for (r, i) in &loc.path {
            a = a.actions.as_ref()?.get(*r)?.actions.get(*i)?;
        }
        Some(a)
    }

    pub fn action_at_mut(&mut self, loc: &Loc) -> Option<&mut ActionData> {
        let p = self.store.get_mut(&loc.profile)?;
        let page = p.pages.get_mut(&loc.page)?;
        let mut a = page.controllers.iter_mut().find(|c| c.kind == loc.ctl.name())?.actions.get_mut(&loc.pos)?;
        for (r, i) in &loc.path {
            a = a.actions.as_mut()?.get_mut(*r)?.actions.get_mut(*i)?;
        }
        self.dirty_pages.insert((loc.profile, loc.page));
        Some(a)
    }

    pub fn ctx_action(&self, ctx: &str) -> Option<&ActionData> {
        let loc = self.instances.get(ctx).and_then(|i| i.loc.clone()).or_else(|| self.locate(ctx))?;
        self.action_at(&loc)
    }

    /// Finds an action anywhere in the store by its context (ActionID). Used
    /// for property inspectors of keys that are not on a device right now.
    pub fn locate(&self, ctx: &str) -> Option<Loc> {
        fn walk(a: &ActionData, ctx: &str, path: &mut Vec<(usize, usize)>) -> bool {
            if a.action_id == ctx {
                return true;
            }
            if let Some(rs) = &a.actions {
                for (ri, r) in rs.iter().enumerate() {
                    for (ai, child) in r.actions.iter().enumerate() {
                        path.push((ri, ai));
                        if walk(child, ctx, path) {
                            return true;
                        }
                        path.pop();
                    }
                }
            }
            false
        }
        for p in self.store.profiles.values() {
            for (pid, page) in &p.pages {
                for c in &page.controllers {
                    let Some(ctl) = Ctl::parse(&c.kind) else { continue };
                    for (pos, a) in &c.actions {
                        let mut path = Vec::new();
                        if walk(a, ctx, &mut path) {
                            return Some(Loc { profile: p.id, page: *pid, ctl, pos: pos.clone(), path });
                        }
                    }
                }
            }
        }
        None
    }

    fn appear(&mut self, dev: &str, slot: Slot, ctx: String, loc: Loc) {
        let Some(action) = self.action_at(&loc).cloned() else { return };
        let def = self.registry.action(&action.uuid);
        let plugin = def.map(|d| d.plugin.clone()).or_else(|| action.plugin.as_ref().map(|p| p.uuid.clone())).unwrap_or_default();
        let builtin = self.registry.plugins.get(&plugin).map(|p| p.builtin).unwrap_or(false);
        let inst = Instance { context: ctx.clone(), device: dev.into(), loc: Some(loc.clone()), slot: Some(slot), uuid: action.uuid.clone(), plugin: plugin.clone(), builtin, ..Default::default() };
        self.instances.insert(ctx.clone(), inst);
        if builtin {
            self.builtin_appear(&ctx);
        } else {
            self.send_will_appear(&ctx, &action, false);
        }
        // Plugin actions inside multi actions and stacks hear willAppear too,
        // flagged as in a multi action, so they can set themselves up.
        if let Some(routines) = &action.actions {
            for (ri, r) in routines.iter().enumerate() {
                for (ai, child) in r.actions.iter().enumerate() {
                    let mut cloc = loc.clone();
                    cloc.path.push((ri, ai));
                    let cplugin = self.registry.action(&child.uuid).map(|d| d.plugin.clone()).unwrap_or_default();
                    let cbuiltin = self.registry.plugins.get(&cplugin).map(|p| p.builtin).unwrap_or(false);
                    let cinst = Instance { context: child.action_id.clone(), device: dev.into(), loc: Some(cloc), slot: Some(slot), uuid: child.uuid.clone(), plugin: cplugin, builtin: cbuiltin, parent: Some(ctx.clone()), ..Default::default() };
                    self.instances.insert(child.action_id.clone(), cinst);
                    if !cbuiltin {
                        self.send_will_appear(&child.action_id, child, true);
                    }
                }
            }
        }
    }

    fn send_will_appear(&mut self, ctx: &str, action: &ActionData, in_multi: bool) {
        let Some(inst) = self.instances.get(ctx) else { return };
        let slot = inst.slot.unwrap_or(Slot::key(0, 0));
        let mut payload = json!({
            "settings": action.settings,
            "coordinates": slot.coordinates(),
            "controller": slot.ctl.name(),
            "state": action.state,
            "isInMultiAction": in_multi,
        });
        if let Some(r) = &action.resources {
            payload["resources"] = r.clone();
        }
        let msg = json!({ "event": "willAppear", "action": action.uuid, "context": ctx, "device": inst.device, "payload": payload });
        let plugin = inst.plugin.clone();
        self.send_plugin(&plugin, msg);
        self.send_title_params(ctx);
    }

    fn send_title_params(&mut self, ctx: &str) {
        let Some(inst) = self.instances.get(ctx) else { return };
        if inst.builtin || inst.parent.is_some() {
            return;
        }
        let Some(a) = self.ctx_action(ctx) else { return };
        let st = a.current_state().cloned().unwrap_or_default();
        let title = inst.title.get(&a.state).cloned().or(st.title.clone()).unwrap_or_default();
        let msg = json!({
            "event": "titleParametersDidChange",
            "action": a.uuid,
            "context": ctx,
            "device": inst.device,
            "payload": {
                "coordinates": inst.slot.map(|s| s.coordinates()),
                "settings": a.settings,
                "state": a.state,
                "title": title,
                "titleParameters": {
                    "fontFamily": st.font_family.unwrap_or_default(),
                    "fontSize": st.font_size.unwrap_or(12.0),
                    "fontStyle": st.font_style.unwrap_or_default(),
                    "fontUnderline": st.font_underline.unwrap_or(false),
                    "showTitle": st.show_title.unwrap_or(true),
                    "titleAlignment": st.title_alignment.unwrap_or_else(|| "bottom".into()),
                    "titleColor": st.title_color.unwrap_or_else(|| "#ffffff".into()),
                }
            }
        });
        let plugin = inst.plugin.clone();
        self.send_plugin(&plugin, msg);
    }

    pub fn disappear(&mut self, ctx: &str) {
        let children: Vec<String> = self.instances.values().filter(|i| i.parent.as_deref() == Some(ctx)).map(|i| i.context.clone()).collect();
        for c in children {
            self.disappear(&c);
        }
        let Some(inst) = self.instances.get(ctx) else { return };
        if inst.builtin {
            self.builtin_disappear(ctx);
        } else if let Some(a) = self.ctx_action(ctx) {
            let msg = json!({
                "event": "willDisappear", "action": a.uuid, "context": ctx, "device": inst.device,
                "payload": { "settings": a.settings, "coordinates": inst.slot.map(|s| s.coordinates()), "controller": inst.slot.map(|s| s.ctl.name()).unwrap_or("Keypad"), "state": a.state, "isInMultiAction": inst.parent.is_some() }
            });
            let plugin = inst.plugin.clone();
            self.send_plugin(&plugin, msg);
        }
        self.instances.remove(ctx);
    }

    /// Re-reads an action after the editor changed it: the instance is torn
    /// down and rebuilt so plugins see willDisappear/willAppear, as they do
    /// in the original app.
    pub fn refresh_ctx_everywhere(&mut self) {
        let devs: Vec<String> = self.devices.keys().cloned().collect();
        for d in devs {
            self.show(&d);
        }
    }

    pub fn reappear_all(&mut self) {
        let devs: Vec<String> = self.devices.keys().cloned().collect();
        for d in &devs {
            let ctxs: Vec<String> = self.devices[d].visible.values().cloned().collect();
            for c in ctxs {
                self.disappear(&c);
            }
            if let Some(dev) = self.devices.get_mut(d) {
                dev.visible.clear();
                dev.last_visual.clear();
            }
        }
        for d in devs {
            self.show(&d);
        }
    }

    // -----------------------------------------------------------------------
    // Rendering
    // -----------------------------------------------------------------------

    pub fn visual_for(&self, ctx: &str) -> KeyVisual {
        let mut v = KeyVisual::default();
        let Some(inst) = self.instances.get(ctx) else { return v };
        let Some(a) = self.ctx_action(ctx) else { return v };
        let loc = inst.loc.clone();
        let st = a.current_state().cloned().unwrap_or_default();
        let def = self.registry.action(&a.uuid);
        let plugin = self.registry.plugins.get(&inst.plugin);
        let mstate = def.and_then(|d| d.states.get(a.state as usize).or_else(|| d.states.first())).cloned().unwrap_or(Value::Null);
        // Image precedence: plugin setImage, then built-in dynamic art, then
        // the user's custom image, then the manifest's state image.
        v.image = inst
            .image
            .get(&a.state)
            .cloned()
            .or_else(|| inst.dyn_image.clone())
            .or_else(|| {
                let rel = st.image.as_deref().filter(|s| !s.is_empty())?;
                if rel.starts_with("data:") || rel.starts_with('<') {
                    return Some(ImageSource::Data(rel.to_string()));
                }
                let loc = loc.as_ref()?;
                self.store.get(&loc.profile)?.resolve_image(&loc.page, rel).map(ImageSource::File)
            })
            .or_else(|| {
                let img = mstate.get("Image").and_then(|i| i.as_str()).filter(|s| !s.is_empty())?;
                let dir = plugin.map(|p| p.dir.clone())?;
                Some(ImageSource::File(render::resolve_plugin_image(&dir, img)))
            })
            .or_else(|| def.and_then(|d| d.icon.clone()).filter(|_| self.unknown_or_missing(&a.uuid)).map(|i| ImageSource::File(render::resolve_plugin_image(std::path::Path::new(""), &i))));
        if self.registry.action(&a.uuid).is_none() {
            // An action from a plugin that is not installed: say so on the key.
            v.overlay = Overlay::Alert;
        }
        let user_title = st.title.clone().filter(|t| !t.is_empty());
        let text = inst.dyn_title.clone().or_else(|| inst.title.get(&a.state).cloned()).or(user_title).or_else(|| mstate.get("Title").and_then(|t| t.as_str()).map(String::from)).unwrap_or_default();
        let show = st.show_title.or_else(|| mstate.get("ShowTitle").and_then(|s| s.as_bool())).unwrap_or(true);
        if show && !text.is_empty() {
            let style = st.font_style.clone().or_else(|| mstate.get("FontStyle").and_then(|s| s.as_str()).map(String::from)).unwrap_or_default().to_lowercase();
            v.title = Some(Title {
                text,
                align: st.title_alignment.clone().or_else(|| mstate.get("TitleAlignment").and_then(|s| s.as_str()).map(String::from)).unwrap_or_else(|| "bottom".into()),
                color: st.title_color.clone().or_else(|| mstate.get("TitleColor").and_then(|s| s.as_str()).map(String::from)).unwrap_or_else(|| "#ffffff".into()),
                family: st.font_family.clone().or_else(|| mstate.get("FontFamily").and_then(|s| s.as_str()).map(String::from)).unwrap_or_default(),
                size: st.font_size.or_else(|| mstate.get("FontSize").and_then(|s| s.as_f64().or_else(|| s.as_str().and_then(|x| x.parse().ok())))).unwrap_or(12.0),
                bold: style.contains("bold"),
                italic: style.contains("italic"),
                underline: st.font_underline.or_else(|| mstate.get("FontUnderline").and_then(|s| s.as_bool())).unwrap_or(false),
                outline: st.outline_thickness.unwrap_or(2.0),
            });
        }
        if let Some((o, until)) = inst.overlay {
            if Instant::now() < until {
                v.overlay = o;
            }
        } else if inst.running {
            v.overlay = Overlay::Running;
        }
        v.progress = inst.progress;
        v
    }

    fn unknown_or_missing(&self, _uuid: &str) -> bool {
        false
    }

    pub fn render_device(&mut self, dev_id: &str) {
        let Some(d) = self.devices.get(dev_id) else { return };
        let spec = d.spec();
        let press_feedback = self.settings.press_feedback;
        let mut jobs = Vec::new();
        let mut new_visuals = Vec::new();
        for row in 0..spec.rows {
            for col in 0..spec.columns {
                let slot = Slot::key(col, row);
                let idx = row * spec.columns + col;
                let mut v = d.visible.get(&slot).map(|ctx| self.visual_for(ctx)).unwrap_or_default();
                if press_feedback && d.pressed.contains_key(&slot) && d.visible.contains_key(&slot) {
                    v.pressed = true;
                }
                if d.asleep {
                    v = KeyVisual::default();
                }
                if d.last_visual.get(&idx) != Some(&v) {
                    new_visuals.push((idx, v.clone()));
                    jobs.push(RenderJob::Key(idx, v));
                }
            }
        }
        let screen = if spec.lcd.is_some() { Some(self.screen_svg(dev_id)) } else { None };
        let d = self.devices.get_mut(dev_id).unwrap();
        for (i, v) in new_visuals {
            d.last_visual.insert(i, v);
        }
        for j in jobs {
            d.tx.send(j).ok();
        }
        if let Some(svg) = screen {
            if d.last_screen.as_ref() != Some(&svg) {
                d.last_screen = Some(svg.clone());
                d.tx.send(RenderJob::Screen(svg)).ok();
            }
        }
    }

    pub fn render_ctx(&mut self, ctx: &str) {
        let Some(inst) = self.instances.get(ctx) else { return };
        // Children of multi actions draw through their parent.
        let target = inst.parent.clone().unwrap_or_else(|| ctx.to_string());
        let dev = self.instances.get(&target).map(|i| i.device.clone());
        if let Some(dev) = dev {
            self.render_device(&dev);
        }
    }

    /// The Plus touch strip is drawn as one SVG with a segment per dial; the
    /// Neo info bar shows the clock and page dots unless an action owns it.
    fn screen_svg(&self, dev_id: &str) -> String {
        let d = &self.devices[dev_id];
        let spec = d.spec();
        let (w, h) = spec.lcd.unwrap();
        if d.asleep {
            return format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}"><rect width="{w}" height="{h}" fill="#000"/></svg>"##);
        }
        if spec.encoders == 0 {
            return self.neo_bar_svg(dev_id, w, h);
        }
        let seg_w = w / spec.encoders as u32;
        let mut body = String::new();
        for i in 0..spec.encoders {
            let slot = Slot::dial(i);
            let seg = match d.visible.get(&slot) {
                Some(ctx) => self.segment_for(ctx, seg_w, h),
                None => format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="{seg_w}" height="{h}"><rect width="{seg_w}" height="{h}" fill="#000"/></svg>"##),
            };
            let inner = seg.replacen("<svg ", &format!(r#"<svg x="{}" y="0" "#, i as u32 * seg_w), 1);
            body.push_str(&inner);
            if i > 0 {
                body.push_str(&format!(r##"<rect x="{}" y="8" width="1" height="{}" fill="#ffffff" fill-opacity="0.12"/>"##, i as u32 * seg_w, h - 16));
            }
        }
        format!(r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{w}" height="{h}" viewBox="0 0 {w} {h}">{body}</svg>"#)
    }

    fn segment_for(&self, ctx: &str, w: u32, h: u32) -> String {
        let Some(inst) = self.instances.get(ctx) else { return String::new() };
        let a = self.ctx_action(ctx);
        let def = a.and_then(|a| self.registry.action(&a.uuid));
        let plugin_dir = self.registry.plugins.get(&inst.plugin).map(|p| p.dir.clone());
        let enc = def.and_then(|d| d.encoder.clone()).unwrap_or(Value::Null);
        let layout = inst
            .layout
            .clone()
            .or_else(|| enc.get("layout").and_then(|l| l.as_str()).and_then(|l| render::load_layout(l, plugin_dir.as_deref())))
            .unwrap_or_else(|| render::builtin_layout("$X1").unwrap());
        let mut fb = Map::new();
        let v = self.visual_for(ctx);
        let title = v.title.as_ref().map(|t| t.text.clone()).or_else(|| def.map(|d| d.name.clone())).unwrap_or_default();
        fb.insert("title".into(), json!({ "value": title }));
        let icon = enc
            .get("Icon")
            .and_then(|i| i.as_str())
            .and_then(|i| plugin_dir.as_ref().map(|d| render::resolve_plugin_image(d, i).to_string_lossy().to_string()))
            .or_else(|| match &v.image {
                Some(ImageSource::File(p)) => Some(p.to_string_lossy().to_string()),
                Some(ImageSource::Data(d)) => Some(d.clone()),
                None => None,
            });
        if let Some(icon) = icon {
            fb.insert("icon".into(), json!({ "value": icon }));
        }
        for (k, val) in &inst.feedback {
            fb.insert(k.clone(), val.clone());
        }
        let background = a
            .and_then(|a| a.encoder.as_ref())
            .and_then(|e| e.get("background").or_else(|| e.get("Background")))
            .and_then(|b| b.as_str())
            .filter(|s| !s.is_empty())
            .and_then(|b| {
                let loc = inst.loc.as_ref()?;
                self.store.get(&loc.profile)?.resolve_image(&loc.page, b).map(ImageSource::File)
            })
            .or_else(|| enc.get("background").and_then(|b| b.as_str()).and_then(|b| plugin_dir.as_ref().map(|d| ImageSource::File(render::resolve_plugin_image(d, b)))));
        let mut svg = render::segment_svg(&layout, &fb, w, h, &render::SegmentContext { plugin_dir: plugin_dir.as_deref(), background });
        if let Some((o, until)) = inst.overlay {
            if Instant::now() < until && o != Overlay::None {
                let color = if o == Overlay::Alert { "#f5c400" } else { "#2fb455" };
                svg = svg.replacen("</svg>", &format!(r#"<rect x="2" y="2" width="{}" height="{}" fill="none" stroke="{color}" stroke-width="4"/></svg>"#, w - 4, h - 4), 1);
            }
        }
        svg
    }

    fn neo_bar_svg(&self, dev_id: &str, w: u32, h: u32) -> String {
        let d = &self.devices[dev_id];
        let pages = d.profile.and_then(|p| self.store.get(&p)).map(|p| p.top_pages()).unwrap_or_default();
        let current = d.folders.first().copied().or(d.page).and_then(|pg| pages.iter().position(|x| *x == pg)).unwrap_or(0);
        let neo_action = d.visible.iter().find(|(s, _)| s.ctl == Ctl::Neo).map(|(_, c)| c.clone());
        let time_fmt = neo_action.as_ref().and_then(|c| self.ctx_action(c)).map(|a| a.setting_bool("24HourFormat", true)).unwrap_or(true);
        let now = chrono::Local::now();
        let time = if time_fmt { now.format("%H:%M").to_string() } else { now.format("%-I:%M %p").to_string() };
        let mut dots = String::new();
        let n = pages.len().max(1);
        let spacing = 12.0;
        let start = w as f64 / 2.0 - (n as f64 - 1.0) * spacing / 2.0;
        for i in 0..n {
            let fill = if i == current { "#ffffff" } else { "#5a5a5a" };
            dots.push_str(&format!(r#"<circle cx="{}" cy="{}" r="3.5" fill="{fill}"/>"#, start + i as f64 * spacing, h as f64 - 10.0));
        }
        format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}"><rect width="{w}" height="{h}" fill="#000"/><text x="{cx}" y="{ty}" font-family="sans-serif" font-size="26" font-weight="600" text-anchor="middle" fill="#fff">{time}</text>{dots}</svg>"##,
            cx = w / 2,
            ty = h as f64 / 2.0 + 4.0,
            time = render::escape(&time)
        )
    }

    pub fn show_overlay(&mut self, ctx: &str, o: Overlay) {
        let target = self.instances.get(ctx).and_then(|i| i.parent.clone()).unwrap_or_else(|| ctx.to_string());
        if let Some(inst) = self.instances.get_mut(&target) {
            inst.overlay = Some((o, Instant::now() + Duration::from_millis(1500)));
        }
        self.render_ctx(&target);
        let tx = self.tx.clone();
        let target2 = target.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(1550)).await;
            tx.send(Msg::Exec(Box::new(move |e: &mut Engine| {
                if let Some(i) = e.instances.get_mut(&target2) {
                    if i.overlay.map(|(_, u)| Instant::now() >= u).unwrap_or(false) {
                        i.overlay = None;
                    }
                }
                e.render_ctx(&target2);
            })))
            .ok();
        });
    }

    pub fn alert(&mut self, ctx: &str, why: &str) {
        tracing::warn!("{ctx}: {why}");
        self.emit("alert", json!({ "context": ctx, "message": why }));
        self.show_overlay(ctx, Overlay::Alert);
    }

    /// Runs async work for an action and shows an alert on the key if it fails.
    pub fn spawn_work(&self, ctx: &str, w: Work) {
        let Some(f) = w else { return };
        let tx = self.tx.clone();
        let ctx = ctx.to_string();
        tokio::spawn(async move {
            if let Err(e) = f.await {
                let msg = format!("{e:#}");
                tx.send(Msg::Exec(Box::new(move |eng: &mut Engine| eng.alert(&ctx, &msg)))).ok();
            }
        });
    }

    // -----------------------------------------------------------------------
    // Input
    // -----------------------------------------------------------------------

    fn on_input(&mut self, dev_id: &str, input: DeviceInput) {
        let Some(d) = self.devices.get_mut(dev_id) else { return };
        d.last_input = Instant::now();
        if d.asleep {
            // The press that wakes the deck does nothing else, as on the original.
            if matches!(input, DeviceInput::KeyDown(_) | DeviceInput::DialDown(_) | DeviceInput::Touch { .. } | DeviceInput::TouchPointDown(_)) {
                self.wake(dev_id);
            }
            return;
        }
        let spec = d.spec();
        match input {
            DeviceInput::KeyDown(i) | DeviceInput::KeyUp(i) => {
                let down = matches!(input, DeviceInput::KeyDown(_));
                let slot = Slot::key(i % spec.columns, i / spec.columns);
                self.key_event(dev_id, slot, down);
            }
            DeviceInput::DialDown(i) => self.dial_event(dev_id, i, DialEv::Down),
            DeviceInput::DialUp(i) => self.dial_event(dev_id, i, DialEv::Up),
            DeviceInput::DialRotate(i, t) => self.dial_event(dev_id, i, DialEv::Rotate(t)),
            DeviceInput::Touch { x, y, long } => {
                let (w, _) = spec.lcd.unwrap_or((800, 100));
                if spec.encoders > 0 {
                    let seg = (w / spec.encoders as u32).max(1);
                    let i = (x as u32 / seg).min(spec.encoders as u32 - 1) as u8;
                    self.dial_event(dev_id, i, DialEv::Touch { x: x - (i as u32 * seg) as u16, y, long });
                }
            }
            DeviceInput::Swipe { from, to } => {
                // Swiping the touch strip turns pages, as on the original.
                if (from.0 as i32 - to.0 as i32).abs() > 40 {
                    self.page_step(dev_id, if to.0 < from.0 { 1 } else { -1 });
                }
            }
            DeviceInput::TouchPointDown(p) => self.page_step(dev_id, if p == 0 { -1 } else { 1 }),
            DeviceInput::TouchPointUp(_) => {}
        }
    }

    pub fn wake(&mut self, dev_id: &str) {
        let b = self.settings.devices.get(dev_id).map(|s| s.brightness).unwrap_or(60);
        if let Some(d) = self.devices.get_mut(dev_id) {
            d.asleep = false;
            d.last_input = Instant::now();
            d.tx.send(RenderJob::Brightness(b)).ok();
        }
        self.render_device(dev_id);
        self.emit_state();
    }

    pub fn sleep_device(&mut self, dev_id: &str) {
        if let Some(d) = self.devices.get_mut(dev_id) {
            d.asleep = true;
            d.tx.send(RenderJob::Brightness(0)).ok();
        }
        self.render_device(dev_id);
        self.emit_state();
    }

    pub fn key_event(&mut self, dev_id: &str, slot: Slot, down: bool) {
        let Some(d) = self.devices.get_mut(dev_id) else { return };
        let ctx = d.visible.get(&slot).cloned();
        let held_for;
        if down {
            d.seq += 1;
            let seq = d.seq;
            d.pressed.insert(slot, (Instant::now(), seq));
            held_for = Duration::ZERO;
            let tx = self.tx.clone();
            let dev = dev_id.to_string();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(500)).await;
                tx.send(Msg::Hold { device: dev, slot, seq }).ok();
            });
        } else {
            held_for = d.pressed.remove(&slot).map(|(t, _)| t.elapsed()).unwrap_or_default();
        }
        self.render_device(dev_id);
        self.emit("keyEvent", json!({ "device": dev_id, "controller": slot.ctl.name(), "position": slot.pos(), "down": down }));
        let Some(ctx) = ctx else { return };
        if down {
            self.trigger_down(&ctx, None);
        } else {
            self.trigger_up(&ctx, held_for, None);
        }
    }

    fn on_hold(&mut self, dev: &str, slot: Slot, seq: u64) {
        let still = self.devices.get(dev).and_then(|d| d.pressed.get(&slot)).map(|(_, s)| *s == seq).unwrap_or(false);
        if !still {
            return;
        }
        let Some(ctx) = self.devices.get(dev).and_then(|d| d.visible.get(&slot)).cloned() else { return };
        self.builtin_hold(&ctx);
    }

    /// Key press for a visible or nested action. `desired_state` is the SDK's
    /// `userDesiredState` for actions run from a multi action.
    pub fn trigger_down(&mut self, ctx: &str, desired_state: Option<u32>) -> Work {
        let Some(inst) = self.instances.get(ctx) else { return None };
        if inst.builtin {
            let w = self.builtin_down(ctx);
            if inst_is_top(self, ctx) {
                self.spawn_work(ctx, w);
                return None;
            }
            return w;
        }
        self.send_key_event(ctx, "keyDown", desired_state);
        None
    }

    pub fn trigger_up(&mut self, ctx: &str, held: Duration, desired_state: Option<u32>) -> Work {
        let Some(inst) = self.instances.get(ctx) else { return None };
        if inst.builtin {
            let w = self.builtin_up(ctx, held);
            if inst_is_top(self, ctx) {
                self.spawn_work(ctx, w);
                return None;
            }
            return w;
        }
        self.send_key_event(ctx, "keyUp", desired_state);
        // Two-state plugin actions flip state on release unless they opt out.
        let auto = self.ctx_action(ctx).and_then(|a| self.registry.action(&a.uuid).map(|d| (d.states.len(), d.disable_automatic_states)));
        if let Some((n, disabled)) = auto {
            if n >= 2 && !disabled {
                let next = desired_state.unwrap_or_else(|| self.ctx_action(ctx).map(|a| (a.state + 1) % n as u32).unwrap_or(0));
                self.set_state(ctx, next);
            }
        }
        None
    }

    fn send_key_event(&mut self, ctx: &str, event: &str, desired: Option<u32>) {
        let Some(inst) = self.instances.get(ctx) else { return };
        let Some(a) = self.ctx_action(ctx) else { return };
        let mut payload = json!({ "settings": a.settings, "coordinates": inst.slot.map(|s| s.coordinates()), "state": a.state, "isInMultiAction": inst.parent.is_some() });
        if let Some(d) = desired {
            payload["userDesiredState"] = json!(d);
        }
        let msg = json!({ "event": event, "action": a.uuid, "context": ctx, "device": inst.device, "payload": payload });
        let plugin = inst.plugin.clone();
        if !self.plugin_conns.contains_key(&plugin) {
            let why = self.registry.plugins.get(&plugin).and_then(|p| p.unsupported.clone()).unwrap_or_else(|| "plugin is not running".into());
            self.alert(ctx, &why);
            return;
        }
        self.send_plugin(&plugin, msg);
    }

    pub fn set_state(&mut self, ctx: &str, state: u32) {
        let Some(loc) = self.instances.get(ctx).and_then(|i| i.loc.clone()).or_else(|| self.locate(ctx)) else { return };
        if let Some(a) = self.action_at_mut(&loc) {
            let max = a.states.len().max(1) as u32;
            a.state = state.min(max - 1);
            while a.states.len() <= a.state as usize {
                a.states.push(Default::default());
            }
        }
        self.render_ctx(ctx);
        self.send_title_params(ctx);
    }

    fn dial_event(&mut self, dev_id: &str, i: u8, ev: DialEv) {
        let slot = Slot::dial(i);
        let Some(ctx) = self.devices.get(dev_id).and_then(|d| d.visible.get(&slot)).cloned() else { return };
        let Some((builtin, plugin)) = self.instances.get(&ctx).map(|i| (i.builtin, i.plugin.clone())) else { return };
        if builtin {
            let w = self.builtin_dial(&ctx, ev);
            self.spawn_work(&ctx, w);
            return;
        }
        let Some(a) = self.ctx_action(&ctx).cloned() else { return };
        let pressed = self.devices.get(dev_id).map(|d| d.pressed.contains_key(&slot)).unwrap_or(false);
        let base = json!({ "settings": a.settings, "coordinates": slot.coordinates(), "controller": "Encoder" });
        let (event, mut payload) = match ev {
            DialEv::Down => {
                self.devices.get_mut(dev_id).unwrap().pressed.insert(slot, (Instant::now(), 0));
                ("dialDown", base)
            }
            DialEv::Up => {
                self.devices.get_mut(dev_id).unwrap().pressed.remove(&slot);
                ("dialUp", base)
            }
            DialEv::Rotate(t) => {
                let mut p = base;
                p["ticks"] = json!(t);
                p["pressed"] = json!(pressed);
                ("dialRotate", p)
            }
            DialEv::Touch { x, y, long } => {
                let mut p = base;
                p["tapPos"] = json!([x, y]);
                p["hold"] = json!(long);
                ("touchTap", p)
            }
        };
        if event == "dialDown" || event == "dialUp" {
            // SDK v1 plugins still listen for dialPress.
            let mut legacy = payload.clone();
            legacy["pressed"] = json!(event == "dialDown");
            let msg = json!({ "event": "dialPress", "action": a.uuid, "context": ctx, "device": dev_id, "payload": legacy });
            self.send_plugin(&plugin, msg);
        }
        payload["state"] = json!(a.state);
        let msg = json!({ "event": event, "action": a.uuid, "context": ctx, "device": dev_id, "payload": payload });
        self.send_plugin(&plugin, msg);
    }

    // -----------------------------------------------------------------------
    // Periodic work
    // -----------------------------------------------------------------------

    fn tick(&mut self) {
        self.flush();
        let now = Instant::now();
        let devs: Vec<String> = self.devices.keys().cloned().collect();
        for d in &devs {
            let mins = self.settings.devices.get(d).map(|s| s.sleep_minutes).unwrap_or(0);
            let (asleep, idle) = {
                let dev = &self.devices[d];
                (dev.asleep, now.duration_since(dev.last_input))
            };
            if mins > 0 && !asleep && idle > Duration::from_secs(mins as u64 * 60) {
                self.sleep_device(d);
            }
        }
        self.builtin_tick();
        // Clocks on the Neo info bar and expiring overlays.
        for d in &devs {
            self.render_device(d);
        }
        self.check_monitored_apps();
        if self.settings.auto_backup && self.last_backup.map(|t| t.elapsed() > Duration::from_secs(6 * 3600)).unwrap_or(true) {
            self.last_backup = Some(now);
            self.auto_backup();
        }
    }

    pub fn flush(&mut self) {
        for (profile, page) in std::mem::take(&mut self.dirty_pages) {
            if let Some(p) = self.store.get(&profile) {
                if let Err(e) = p.save_page(&page) {
                    tracing::warn!("saving page: {e}");
                }
            }
        }
        for profile in std::mem::take(&mut self.dirty_manifests) {
            if let Some(p) = self.store.get(&profile) {
                p.save_manifest().ok();
            }
        }
    }

    pub fn mark_dirty(&mut self, profile: Uuid, page: Uuid) {
        self.dirty_pages.insert((profile, page));
    }

    pub fn mark_manifest_dirty(&mut self, profile: Uuid) {
        self.dirty_manifests.insert(profile);
    }

    fn auto_backup(&mut self) {
        let dir = crate::paths::backups_dir();
        let out = dir.join(format!("auto-{}.streamDeckProfilesBackup", chrono::Local::now().format("%Y%m%d-%H%M%S")));
        if let Err(e) = self.store.backup(&out) {
            tracing::warn!("backup: {e}");
            return;
        }
        let mut autos: Vec<PathBuf> = std::fs::read_dir(&dir).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.file_name().map(|n| n.to_string_lossy().starts_with("auto-")).unwrap_or(false)).collect()).unwrap_or_default();
        autos.sort();
        while autos.len() > self.settings.backups_to_keep.max(1) {
            std::fs::remove_file(autos.remove(0)).ok();
        }
    }

    // -----------------------------------------------------------------------
    // Smart profiles and monitored applications
    // -----------------------------------------------------------------------

    fn on_hypr(&mut self, e: HyprEvent) {
        match e {
            HyprEvent::ActiveClass(class) => {
                if class == self.active_class {
                    return;
                }
                self.active_class = class.clone();
                if self.settings.smart_profiles {
                    self.apply_smart_profiles(&class);
                }
                self.emit("activeApp", json!({ "class": class }));
            }
            HyprEvent::WindowsChanged => {
                self.check_monitored_apps();
                self.builtin_windows_changed();
            }
        }
    }

    fn app_matches(&self, ident: &str, class: &str) -> bool {
        if ident.is_empty() || class.is_empty() {
            return false;
        }
        let class = class.to_lowercase();
        if let Some(alias) = self.settings.app_aliases.get(ident) {
            return alias.to_lowercase() == class;
        }
        let ident_l = ident.to_lowercase();
        if ident_l == class {
            return true;
        }
        // macOS bundle paths ("/Applications/Firefox.app") and desktop files
        // reduce to the app name, which usually is the window class.
        let stem = std::path::Path::new(&ident_l).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let stem = stem.trim_end_matches(".desktop").to_string();
        stem == class || stem.rsplit('.').next() == Some(class.as_str()) || stem.replace(' ', "-") == class
    }

    fn apply_smart_profiles(&mut self, class: &str) {
        let devs: Vec<String> = self.devices.keys().cloned().collect();
        for d in devs {
            let spec = self.devices[&d].spec();
            let target = self
                .store
                .for_device(&d, spec)
                .find(|p| p.manifest.app_identifier.as_deref().map(|i| self.app_matches(i, class)).unwrap_or(false))
                .map(|p| p.id);
            let dev = &self.devices[&d];
            match target {
                Some(t) if dev.profile != Some(t) => {
                    let ret = dev.smart_return.or(dev.profile);
                    self.switch_profile(&d, t, None);
                    if let Some(dev) = self.devices.get_mut(&d) {
                        dev.smart_return = ret;
                    }
                }
                None => {
                    if let Some(ret) = dev.smart_return {
                        self.switch_profile(&d, ret, None);
                        if let Some(dev) = self.devices.get_mut(&d) {
                            dev.smart_return = None;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// `ApplicationsToMonitor` in a plugin manifest lists apps whose launch
    /// and exit the plugin wants to hear about. On Linux the names are
    /// matched against process names (`linux` list, falling back to the
    /// macOS bundle ids' last component).
    fn check_monitored_apps(&mut self) {
        let mut watched: Vec<(String, String)> = Vec::new();
        for p in self.registry.plugins.values() {
            let Some(atm) = p.manifest.get("ApplicationsToMonitor") else { continue };
            let names: Vec<String> = ["linux", "mac", "windows"].iter().filter_map(|k| atm.get(*k)).filter_map(|v| v.as_array()).flatten().filter_map(|v| v.as_str()).map(String::from).collect();
            for n in names {
                watched.push((p.uuid.clone(), n));
            }
        }
        if watched.is_empty() {
            return;
        }
        let running = crate::system::running_process_names();
        let norm = |n: &str| -> String {
            let base = n.rsplit(['.', '/', '\\']).next().unwrap_or(n);
            let base = if n.ends_with(".exe") { n.trim_end_matches(".exe").rsplit(['\\', '/']).next().unwrap_or(n) } else { base };
            base.to_lowercase()
        };
        let running_l: HashSet<String> = running.iter().map(|r| r.to_lowercase()).collect();
        for (plugin, app) in watched {
            let key = format!("{plugin}|{app}");
            let is = running_l.contains(&norm(&app));
            let was = self.monitored_running.contains(&key);
            if is != was {
                if is {
                    self.monitored_running.insert(key);
                } else {
                    self.monitored_running.remove(&key);
                }
                let event = if is { "applicationDidLaunch" } else { "applicationDidTerminate" };
                self.send_plugin(&plugin, json!({ "event": event, "payload": { "application": app } }));
            }
        }
    }

    // -----------------------------------------------------------------------
    // Plugins
    // -----------------------------------------------------------------------

    pub fn start_all_plugins(&mut self) {
        let ids: Vec<String> = self.registry.plugins.values().filter(|p| !p.builtin && p.unsupported.is_none()).map(|p| p.uuid.clone()).collect();
        for id in ids {
            self.start_plugin(&id);
        }
    }

    pub fn start_plugin(&mut self, uuid: &str) {
        if self.running.contains_key(uuid) || self.stopped_plugins.contains(uuid) {
            return;
        }
        if !self.registry.plugins.contains_key(uuid) {
            return;
        }
        self.install_plugin_profiles(uuid);
        let info = info_payload(self, uuid);
        let Some(p) = self.registry.plugins.get(uuid) else { return };
        match plugins::spawn(p, self.sdk_port, &info) {
            Ok(child) => {
                let restarts = self.running.get(uuid).map(|r| r.restarts).unwrap_or(0);
                self.running.insert(uuid.to_string(), RunningPlugin { child, started: Instant::now(), restarts });
                self.watch_plugin_exit(uuid);
            }
            Err(e) => {
                tracing::warn!("{uuid}: {e:#}");
                if let Some(p) = self.registry.plugins.get_mut(uuid) {
                    p.unsupported = Some(format!("{e:#}"));
                }
            }
        }
    }

    fn watch_plugin_exit(&mut self, uuid: &str) {
        let Some(r) = self.running.get_mut(uuid) else { return };
        let Some(pid) = r.child.id() else { return };
        let tx = self.tx.clone();
        let uuid = uuid.to_string();
        // Poll for exit without taking the Child out of the map.
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(500)).await;
                let alive = unsafe { libc::kill(pid as i32, 0) } == 0 && !is_zombie(pid);
                if !alive {
                    tx.send(Msg::PluginExited(uuid)).ok();
                    break;
                }
            }
        });
    }

    fn on_plugin_exit(&mut self, uuid: &str) {
        let Some(mut r) = self.running.remove(uuid) else { return };
        let status = r.child.try_wait().ok().flatten();
        tracing::warn!("plugin {uuid} exited ({status:?})");
        if self.stopped_plugins.contains(uuid) {
            return;
        }
        match plugins::should_restart(r.started, r.restarts) {
            Some(delay) => {
                let tx = self.tx.clone();
                let uuid2 = uuid.to_string();
                let restarts = if r.started.elapsed() > Duration::from_secs(60) { 0 } else { r.restarts + 1 };
                tokio::spawn(async move {
                    tokio::time::sleep(delay).await;
                    tx.send(Msg::Exec(Box::new(move |e: &mut Engine| {
                        e.start_plugin(&uuid2);
                        if let Some(r) = e.running.get_mut(&uuid2) {
                            r.restarts = restarts;
                        }
                    })))
                    .ok();
                });
            }
            None => {
                self.emit("pluginCrashed", json!({ "plugin": uuid }));
                if let Some(p) = self.registry.plugins.get_mut(uuid) {
                    p.unsupported = Some("stopped after crashing repeatedly; see its log".into());
                }
            }
        }
    }

    pub fn stop_plugin(&mut self, uuid: &str) {
        self.stopped_plugins.insert(uuid.to_string());
        if let Some(mut r) = self.running.remove(uuid) {
            r.child.start_kill().ok();
        }
        if let Some((conn, _)) = self.plugin_conns.remove(uuid) {
            self.conn_index.remove(&conn);
        }
    }

    pub fn restart_plugin(&mut self, uuid: &str) {
        self.stop_plugin(uuid);
        self.stopped_plugins.remove(uuid);
        if let Some(p) = self.registry.plugins.get_mut(uuid) {
            p.unsupported = None;
        }
        self.start_plugin(uuid);
    }

    pub fn running_plugins(&self) -> Vec<String> {
        self.plugin_conns.keys().cloned().collect()
    }

    /// Imports the profiles a plugin bundles (manifest `Profiles`) once.
    fn install_plugin_profiles(&mut self, uuid: &str) {
        let Some(p) = self.registry.plugins.get(uuid) else { return };
        let Some(list) = p.manifest.get("Profiles").and_then(|l| l.as_array()).cloned() else { return };
        let dir = p.dir.clone();
        for entry in list {
            let name = entry.get("Name").and_then(|n| n.as_str()).unwrap_or("").to_string();
            if name.is_empty() {
                continue;
            }
            let exists = self.store.profiles.values().any(|pr| pr.manifest.extra.get("InstalledByPluginUUID").and_then(|v| v.as_str()) == Some(uuid) && pr.manifest.extra.get("PreconfiguredName").and_then(|v| v.as_str()) == Some(&name));
            if exists {
                continue;
            }
            let file = dir.join(format!("{name}.streamDeckProfile"));
            if !file.exists() {
                continue;
            }
            match self.store.import(&file, None) {
                Ok(ids) => {
                    for id in ids {
                        if let Some(pr) = self.store.get_mut(&id) {
                            pr.manifest.extra.insert("InstalledByPluginUUID".into(), json!(uuid));
                            pr.manifest.extra.insert("PreconfiguredName".into(), json!(name));
                            pr.save_manifest().ok();
                        }
                    }
                }
                Err(e) => tracing::warn!("{uuid}: bundled profile {name}: {e:#}"),
            }
        }
    }

    pub fn send_plugin(&mut self, plugin: &str, msg: Value) {
        if let Some((_, tx)) = self.plugin_conns.get(plugin) {
            tx.send(msg.to_string()).ok();
        }
    }

    fn broadcast_plugins(&mut self, msg: Value) {
        let text = msg.to_string();
        for (_, tx) in self.plugin_conns.values() {
            tx.send(text.clone()).ok();
        }
    }

    fn send_pi(&mut self, ctx: &str, msg: Value) {
        if let Some((_, tx)) = self.pi_conns.get(ctx) {
            tx.send(msg.to_string()).ok();
        }
    }

    fn global_settings(&mut self, plugin: &str) -> Value {
        self.global_settings.entry(plugin.to_string()).or_insert_with(|| settings::load_global_settings(plugin)).clone()
    }

    fn on_sdk(&mut self, e: SdkEvent) {
        match e {
            SdkEvent::Registered { conn, kind, id, tx } => {
                self.conn_index.insert(conn, (kind, id.clone()));
                match kind {
                    ConnKind::Plugin => {
                        tracing::info!("plugin {id} registered");
                        self.plugin_conns.insert(id.clone(), (conn, tx));
                        // Bring the plugin up to date with what is on screen.
                        let devices: Vec<Value> = self.devices.values().filter(|d| d.connected).map(|d| json!({ "event": "deviceDidConnect", "device": d.info.id, "deviceInfo": device_info_json(d) })).collect();
                        for m in devices {
                            self.send_plugin(&id, m);
                        }
                        let ctxs: Vec<String> = self.instances.values().filter(|i| i.plugin == id).map(|i| i.context.clone()).collect();
                        for c in ctxs {
                            if let Some(a) = self.ctx_action(&c).cloned() {
                                let in_multi = self.instances.get(&c).map(|i| i.parent.is_some()).unwrap_or(false);
                                self.send_will_appear(&c, &a, in_multi);
                            }
                        }
                        self.emit("pluginsChanged", json!({}));
                    }
                    ConnKind::PropertyInspector => {
                        self.pi_conns.insert(id.clone(), (conn, tx));
                        if let Some(a) = self.ctx_action(&id).cloned() {
                            let plugin = self.registry.action(&a.uuid).map(|d| d.plugin.clone()).unwrap_or_default();
                            let device = self.instances.get(&id).map(|i| i.device.clone()).unwrap_or_default();
                            self.send_plugin(&plugin, json!({ "event": "propertyInspectorDidAppear", "action": a.uuid, "context": id, "device": device }));
                        }
                    }
                }
            }
            SdkEvent::Closed { conn } => {
                let Some((kind, id)) = self.conn_index.remove(&conn) else { return };
                match kind {
                    ConnKind::Plugin => {
                        if self.plugin_conns.get(&id).map(|(c, _)| *c == conn).unwrap_or(false) {
                            self.plugin_conns.remove(&id);
                        }
                    }
                    ConnKind::PropertyInspector => {
                        if self.pi_conns.get(&id).map(|(c, _)| *c == conn).unwrap_or(false) {
                            self.pi_conns.remove(&id);
                            if let Some(a) = self.ctx_action(&id).cloned() {
                                let plugin = self.registry.action(&a.uuid).map(|d| d.plugin.clone()).unwrap_or_default();
                                let device = self.instances.get(&id).map(|i| i.device.clone()).unwrap_or_default();
                                self.send_plugin(&plugin, json!({ "event": "propertyInspectorDidDisappear", "action": a.uuid, "context": id, "device": device }));
                            }
                        }
                    }
                }
            }
            SdkEvent::Message { conn, value } => {
                let Some((kind, id)) = self.conn_index.get(&conn).cloned() else { return };
                if let Err(e) = self.on_sdk_message(kind, &id, value) {
                    tracing::debug!("sdk message from {id}: {e:#}");
                }
            }
        }
    }

    fn on_sdk_message(&mut self, kind: ConnKind, from: &str, v: Value) -> Result<()> {
        let event = v.get("event").and_then(|e| e.as_str()).unwrap_or("").to_string();
        let context = v.get("context").and_then(|c| c.as_str()).unwrap_or("").to_string();
        let payload = v.get("payload").cloned().unwrap_or(Value::Null);
        // The plugin that owns a context, used to route PI traffic.
        let plugin_of = |e: &Engine, ctx: &str| -> String {
            if kind == ConnKind::Plugin {
                return from.to_string();
            }
            e.ctx_action(ctx).and_then(|a| e.registry.action(&a.uuid)).map(|d| d.plugin.clone()).unwrap_or_default()
        };
        match event.as_str() {
            "setSettings" => {
                let loc = self.instances.get(&context).and_then(|i| i.loc.clone()).or_else(|| self.locate(&context)).ok_or_else(|| anyhow!("unknown context"))?;
                if let Some(a) = self.action_at_mut(&loc) {
                    a.settings = payload.clone();
                }
                let a = self.action_at(&loc).cloned().ok_or_else(|| anyhow!("gone"))?;
                let device = self.instances.get(&context).map(|i| i.device.clone()).unwrap_or_default();
                let coords = self.instances.get(&context).and_then(|i| i.slot).map(|s| s.coordinates());
                let notice = json!({ "event": "didReceiveSettings", "action": a.uuid, "context": context, "device": device, "payload": { "settings": payload, "coordinates": coords, "state": a.state, "isInMultiAction": !loc.path.is_empty() } });
                if kind == ConnKind::Plugin {
                    self.send_pi(&context, notice);
                } else {
                    let plugin = plugin_of(self, &context);
                    self.send_plugin(&plugin, notice);
                }
                self.emit("settingsChanged", json!({ "context": context }));
                self.render_ctx(&context);
            }
            "getSettings" => {
                let a = self.ctx_action(&context).cloned().ok_or_else(|| anyhow!("unknown context"))?;
                let device = self.instances.get(&context).map(|i| i.device.clone()).unwrap_or_default();
                let coords = self.instances.get(&context).and_then(|i| i.slot).map(|s| s.coordinates());
                let msg = json!({ "event": "didReceiveSettings", "action": a.uuid, "context": context, "device": device, "payload": { "settings": a.settings, "coordinates": coords, "state": a.state, "isInMultiAction": self.instances.get(&context).map(|i| i.parent.is_some()).unwrap_or(false) } });
                if kind == ConnKind::Plugin {
                    self.send_plugin(from, msg);
                } else {
                    self.send_pi(&context, msg);
                }
            }
            "setGlobalSettings" => {
                let plugin = if kind == ConnKind::Plugin { from.to_string() } else { plugin_of(self, &context) };
                self.global_settings.insert(plugin.clone(), payload.clone());
                settings::save_global_settings(&plugin, &payload);
                let msg = json!({ "event": "didReceiveGlobalSettings", "payload": { "settings": payload } });
                // The other side hears about it: PIs when the plugin wrote, the plugin when a PI did.
                if kind == ConnKind::PropertyInspector {
                    self.send_plugin(&plugin, msg.clone());
                }
                let pis: Vec<String> = self.pi_conns.keys().filter(|c| plugin_of(self, c) == plugin && (kind == ConnKind::Plugin || **c != context)).cloned().collect();
                for c in pis {
                    self.send_pi(&c, msg.clone());
                }
            }
            "getGlobalSettings" => {
                let plugin = if kind == ConnKind::Plugin { from.to_string() } else { plugin_of(self, &context) };
                let gs = self.global_settings(&plugin);
                let msg = json!({ "event": "didReceiveGlobalSettings", "payload": { "settings": gs } });
                if kind == ConnKind::Plugin {
                    self.send_plugin(from, msg);
                } else {
                    self.send_pi(&context, msg);
                }
            }
            "openUrl" => {
                if let Some(url) = payload.get("url").and_then(|u| u.as_str()) {
                    if url.starts_with("http://") || url.starts_with("https://") || url.starts_with("mailto:") {
                        crate::system::spawn_detached("xdg-open", &[url.to_string()])?;
                    }
                }
            }
            "logMessage" => {
                let m = payload.get("message").and_then(|m| m.as_str()).unwrap_or("");
                tracing::info!(target: "plugin", "[{from}] {m}");
            }
            "setTitle" => {
                let target = payload.get("target").and_then(|t| t.as_i64()).unwrap_or(0);
                if target == 2 {
                    return Ok(()); // software only: our editor shows the device's title anyway
                }
                let title = payload.get("title").and_then(|t| t.as_str()).map(String::from);
                let state = payload.get("state").and_then(|s| s.as_u64()).map(|s| s as u32);
                let n = self.ctx_action(&context).map(|a| a.states.len().max(1)).unwrap_or(1) as u32;
                if let Some(inst) = self.instances.get_mut(&context) {
                    let states: Vec<u32> = match state {
                        Some(s) => vec![s],
                        None => (0..n).collect(),
                    };
                    for s in states {
                        match &title {
                            Some(t) => {
                                inst.title.insert(s, t.clone());
                            }
                            None => {
                                inst.title.remove(&s);
                            }
                        }
                    }
                }
                self.render_ctx(&context);
                self.send_title_params(&context);
            }
            "setImage" => {
                let target = payload.get("target").and_then(|t| t.as_i64()).unwrap_or(0);
                if target == 2 {
                    return Ok(());
                }
                let image = payload.get("image").and_then(|t| t.as_str()).filter(|s| !s.is_empty()).map(String::from);
                let state = payload.get("state").and_then(|s| s.as_u64()).map(|s| s as u32);
                let n = self.ctx_action(&context).map(|a| a.states.len().max(1)).unwrap_or(1) as u32;
                let plugin_dir = self.registry.plugins.get(&plugin_of(self, &context)).map(|p| p.dir.clone());
                let src = image.map(|i| {
                    if i.starts_with("data:") || i.trim_start().starts_with('<') {
                        ImageSource::Data(i)
                    } else {
                        ImageSource::File(render::resolve_plugin_image(plugin_dir.as_deref().unwrap_or(std::path::Path::new(".")), &i))
                    }
                });
                if let Some(inst) = self.instances.get_mut(&context) {
                    let states: Vec<u32> = match state {
                        Some(s) => vec![s],
                        None => (0..n).collect(),
                    };
                    for s in states {
                        match &src {
                            Some(img) => {
                                inst.image.insert(s, img.clone());
                            }
                            None => {
                                inst.image.remove(&s);
                            }
                        }
                    }
                }
                self.render_ctx(&context);
            }
            "showAlert" => self.show_overlay(&context, Overlay::Alert),
            "showOk" => self.show_overlay(&context, Overlay::Ok),
            "setState" => {
                let s = payload.get("state").and_then(|s| s.as_u64()).unwrap_or(0) as u32;
                self.set_state(&context, s);
            }
            "switchToProfile" => {
                let device = v.get("device").and_then(|d| d.as_str()).unwrap_or("").to_string();
                let name = payload.get("profile").and_then(|p| p.as_str()).unwrap_or("").to_string();
                let page = payload.get("page").and_then(|p| p.as_u64());
                let target = self
                    .store
                    .profiles
                    .values()
                    .find(|p| p.manifest.extra.get("InstalledByPluginUUID").and_then(|v| v.as_str()) == Some(from) && (p.manifest.extra.get("PreconfiguredName").and_then(|v| v.as_str()) == Some(&name) || p.manifest.name == name))
                    .map(|p| (p.id, p.top_pages()));
                let device = if self.devices.contains_key(&device) { device } else { self.devices.keys().next().cloned().unwrap_or_default() };
                match target {
                    Some((id, pages)) => {
                        let pg = page.and_then(|i| pages.get(i as usize).copied());
                        self.switch_profile(&device, id, pg);
                    }
                    None if name.is_empty() => {
                        // An empty profile name returns to the user's profile.
                        if let Some(p) = self.settings.devices.get(&device).and_then(|s| s.profile.clone()).and_then(|p| p.parse().ok()) {
                            self.switch_profile(&device, p, None);
                        }
                    }
                    None => tracing::warn!("{from} asked for unknown profile {name}"),
                }
            }
            "sendToPropertyInspector" => {
                let action = self.ctx_action(&context).map(|a| a.uuid.clone()).unwrap_or_default();
                self.send_pi(&context, json!({ "event": "sendToPropertyInspector", "action": action, "context": context, "payload": payload }));
            }
            "sendToPlugin" => {
                let plugin = plugin_of(self, &context);
                let action = v.get("action").cloned().unwrap_or_else(|| json!(self.ctx_action(&context).map(|a| a.uuid.clone())));
                self.send_plugin(&plugin, json!({ "event": "sendToPlugin", "action": action, "context": context, "payload": payload }));
            }
            "setFeedback" => {
                if let Some(inst) = self.instances.get_mut(&context) {
                    render::merge_feedback(&mut inst.feedback, &payload);
                }
                self.render_ctx(&context);
            }
            "setFeedbackLayout" => {
                let layout = payload.get("layout").and_then(|l| l.as_str()).unwrap_or("");
                let dir = self.registry.plugins.get(&plugin_of(self, &context)).map(|p| p.dir.clone());
                let l = render::load_layout(layout, dir.as_deref());
                if l.is_none() {
                    self.alert(&context, &format!("unknown layout {layout}"));
                }
                if let Some(inst) = self.instances.get_mut(&context) {
                    inst.layout = l;
                    inst.feedback.clear();
                }
                self.render_ctx(&context);
            }
            "setTriggerDescription" => {
                if let Some(inst) = self.instances.get_mut(&context) {
                    inst.trigger = Some(payload);
                }
                self.emit("settingsChanged", json!({ "context": context }));
            }
            "getSecrets" => {
                // Marketplace secrets only exist for plugins sold through Elgato's store.
                self.send_plugin(from, json!({ "event": "didReceiveSecrets", "payload": { "secrets": {} } }));
            }
            "setResources" | "getResources" => {
                let loc = self.instances.get(&context).and_then(|i| i.loc.clone()).or_else(|| self.locate(&context)).ok_or_else(|| anyhow!("unknown context"))?;
                if event == "setResources" {
                    if let Some(a) = self.action_at_mut(&loc) {
                        a.resources = Some(payload.clone());
                    }
                }
                let a = self.action_at(&loc).cloned().ok_or_else(|| anyhow!("gone"))?;
                let msg = json!({ "event": "didReceiveResources", "action": a.uuid, "context": context, "payload": { "resources": a.resources.unwrap_or(json!({})) } });
                if kind == ConnKind::Plugin {
                    self.send_plugin(from, msg.clone());
                } else {
                    self.send_pi(&context, msg.clone());
                }
            }
            other => tracing::debug!("unhandled SDK event {other} from {from}"),
        }
        Ok(())
    }

    /// `streamdeck://plugins/message/<plugin uuid>/...` deep links.
    pub fn deep_link(&mut self, url: &str) -> Result<()> {
        let rest = url.strip_prefix("streamdeck://").ok_or_else(|| anyhow!("not a streamdeck:// URL"))?;
        let parts: Vec<&str> = rest.splitn(4, '/').collect();
        if parts.len() >= 3 && parts[0] == "plugins" && parts[1] == "message" {
            let plugin = parts[2].split(['?', '#']).next().unwrap_or("").to_string();
            if !self.plugin_conns.contains_key(&plugin) {
                anyhow::bail!("plugin {plugin} is not running");
            }
            self.send_plugin(&plugin, json!({ "event": "didReceiveDeepLink", "payload": { "url": url } }));
            return Ok(());
        }
        anyhow::bail!("unsupported deep link {url}")
    }
}

fn inst_is_top(e: &Engine, ctx: &str) -> bool {
    e.instances.get(ctx).map(|i| i.parent.is_none()).unwrap_or(true)
}

fn is_zombie(pid: u32) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/stat")).map(|s| s.rsplit(')').next().map(|r| r.trim_start().starts_with('Z')).unwrap_or(false)).unwrap_or(true)
}

#[derive(Clone, Copy, Debug)]
pub enum DialEv {
    Down,
    Up,
    Rotate(i32),
    Touch { x: u16, y: u16, long: bool },
}

pub fn page_label(id: &Uuid) -> String {
    page_dir_name(id)
}

pub fn spec_for(e: &Engine, dev: &str) -> Option<&'static DeviceSpec> {
    e.devices.get(dev).map(|d| d.spec()).or_else(|| catalog::spec(dev))
}
