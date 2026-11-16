//! Control API on a Unix socket: newline-delimited JSON. Requests are
//! `{"id": …, "method": "…", "params": {…}}` and get `{"id", "result"}` or
//! `{"id", "error"}` back. After `subscribe` a client also receives
//! `{"event": "…", "data": …}` lines; after `subscribeFrames` it receives a
//! `frame` event with a base64 PNG whenever a key or screen is redrawn.
//! docs/CONTROL_API.md lists every method.

use crate::device::{FrameStore, Preview};
use crate::engine::{Ctl, Engine, Loc, Msg, Slot};
use crate::model::ActionData;
use crate::plugins;
use crate::store;
use anyhow::{anyhow, bail, Result};
use base64::Engine as _;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;

static CLIENTS: AtomicU64 = AtomicU64::new(1);

pub async fn serve(path: PathBuf, engine: mpsc::UnboundedSender<Msg>, previews: broadcast::Sender<Preview>, frames: FrameStore) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    if path.exists() {
        // A live daemon would answer; a stale socket file would not.
        if tokio::net::UnixStream::connect(&path).await.is_ok() {
            bail!("another streamdeckd is already running ({})", path.display());
        }
        std::fs::remove_file(&path).ok();
    }
    let listener = UnixListener::bind(&path)?;
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).ok();
    }
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else { continue };
            let engine = engine.clone();
            let previews = previews.clone();
            let frames = frames.clone();
            tokio::spawn(async move {
                let client = CLIENTS.fetch_add(1, Ordering::Relaxed);
                let (read, mut write) = stream.into_split();
                let (tx, mut rx) = mpsc::unbounded_channel::<String>();
                let writer = tokio::spawn(async move {
                    while let Some(mut line) = rx.recv().await {
                        line.push('\n');
                        if write.write_all(line.as_bytes()).await.is_err() {
                            break;
                        }
                    }
                });
                let mut frame_task: Option<tokio::task::JoinHandle<()>> = None;
                let mut lines = BufReader::new(read).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if line.trim().is_empty() {
                        continue;
                    }
                    let req: Value = match serde_json::from_str(&line) {
                        Ok(v) => v,
                        Err(e) => {
                            tx.send(json!({ "id": null, "error": format!("bad JSON: {e}") }).to_string()).ok();
                            continue;
                        }
                    };
                    let id = req.get("id").cloned().unwrap_or(Value::Null);
                    let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("").to_string();
                    let params = req.get("params").cloned().unwrap_or(json!({}));
                    match method.as_str() {
                        "subscribe" => {
                            engine.send(Msg::Subscribe { client, tx: tx.clone() }).ok();
                            tx.send(json!({ "id": id, "result": true }).to_string()).ok();
                        }
                        "subscribeFrames" => {
                            let device = params.get("device").and_then(|d| d.as_str()).map(String::from);
                            // Replay the current frames, then stream changes.
                            for ((dev, target, index), png) in frames.lock().unwrap().iter() {
                                if device.as_ref().map(|d| d == dev).unwrap_or(true) {
                                    tx.send(frame_event(dev, target, *index, png)).ok();
                                }
                            }
                            let mut rx = previews.subscribe();
                            let tx2 = tx.clone();
                            if let Some(t) = frame_task.take() {
                                t.abort();
                            }
                            frame_task = Some(tokio::spawn(async move {
                                loop {
                                    match rx.recv().await {
                                        Ok(p) => {
                                            if device.as_ref().map(|d| *d == p.device).unwrap_or(true) && tx2.send(frame_event(&p.device, p.target, p.index, &p.png)).is_err() {
                                                break;
                                            }
                                        }
                                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                                        Err(_) => break,
                                    }
                                }
                            }));
                            tx.send(json!({ "id": id, "result": true }).to_string()).ok();
                        }
                        "getFrames" => {
                            let device = params.get("device").and_then(|d| d.as_str()).unwrap_or("");
                            let list: Vec<Value> = frames
                                .lock()
                                .unwrap()
                                .iter()
                                .filter(|((d, _, _), _)| d == device)
                                .map(|((_, t, i), png)| json!({ "target": t, "index": i, "png": base64::engine::general_purpose::STANDARD.encode(png.as_slice()) }))
                                .collect();
                            tx.send(json!({ "id": id, "result": list }).to_string()).ok();
                        }
                        _ => {
                            engine.send(Msg::Control { id, method, params, reply: tx.clone() }).ok();
                        }
                    }
                }
                if let Some(t) = frame_task {
                    t.abort();
                }
                engine.send(Msg::Unsubscribe(client)).ok();
                drop(tx);
                writer.await.ok();
            });
        }
    });
    Ok(())
}

fn frame_event(device: &str, target: &str, index: u8, png: &[u8]) -> String {
    json!({ "event": "frame", "data": { "device": device, "target": target, "index": index, "png": base64::engine::general_purpose::STANDARD.encode(png) } }).to_string()
}

/// Mirrors every frame into `$XDG_RUNTIME_DIR/streamdeck-linux/frames/` so
/// shell widgets can show keys with a plain image element.
pub fn spawn_frame_files(previews: &broadcast::Sender<Preview>) {
    let mut rx = previews.subscribe();
    let root = crate::paths::runtime_dir().join("frames");
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(p) => {
                    let dir = root.join(sanitize(&p.device));
                    std::fs::create_dir_all(&dir).ok();
                    let file = dir.join(format!("{}-{}.png", p.target, p.index));
                    let tmp = file.with_extension("png.tmp");
                    if std::fs::write(&tmp, p.png.as_slice()).is_ok() {
                        std::fs::rename(&tmp, &file).ok();
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            }
        }
    });
}

pub fn sanitize(s: &str) -> String {
    s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
}

fn p_str<'a>(p: &'a Value, k: &str) -> Result<&'a str> {
    p.get(k).and_then(|v| v.as_str()).ok_or_else(|| anyhow!("missing parameter `{k}`"))
}

fn p_uuid(p: &Value, k: &str) -> Result<Uuid> {
    p_str(p, k)?.parse().map_err(|_| anyhow!("`{k}` is not a UUID"))
}

fn loc_from(p: &Value) -> Result<Loc> {
    let ctl = Ctl::parse(p.get("controller").and_then(|c| c.as_str()).unwrap_or("Keypad")).ok_or_else(|| anyhow!("bad controller"))?;
    let path: Vec<(usize, usize)> = p.get("path").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|x| Some((x.get(0)?.as_u64()? as usize, x.get(1)?.as_u64()? as usize))).collect()).unwrap_or_default();
    Ok(Loc { profile: p_uuid(p, "profile")?, page: p_uuid(p, "page")?, ctl, pos: p_str(p, "position")?.to_string(), path })
}

impl Engine {
    pub fn handle_control(&mut self, method: &str, p: Value) -> Result<Value> {
        let r = match method {
            "ping" => json!("pong"),
            "getState" => self.state_json(),
            "listPlugins" => {
                let running = self.running_plugins();
                let list: Vec<Value> = self
                    .registry
                    .plugins
                    .values()
                    .map(|pl| {
                        let mut v = serde_json::to_value(pl).unwrap_or(json!({}));
                        v["running"] = json!(pl.builtin || running.contains(&pl.uuid));
                        v
                    })
                    .collect();
                json!(list)
            }
            "listProfiles" => {
                let device = p.get("device").and_then(|d| d.as_str());
                let list: Vec<Value> = match device.and_then(|d| self.devices.get(d).map(|dev| (d, dev.spec()))) {
                    Some((d, spec)) => self.store.for_device(d, spec).map(|pr| pr.summary()).collect(),
                    None => self.store.profiles.values().map(|pr| pr.summary()).collect(),
                };
                json!(list)
            }
            "getProfile" => {
                let id = p_uuid(&p, "profile")?;
                let pr = self.store.get(&id).ok_or_else(|| anyhow!("unknown profile"))?;
                let pages: serde_json::Map<String, Value> = pr
                    .pages
                    .iter()
                    .map(|(pid, data)| {
                        let mut v = serde_json::to_value(data).unwrap_or(json!({}));
                        v["_dir"] = json!(pr.page_dir(pid).to_string_lossy());
                        v["_folder"] = json!(pr.is_folder(pid));
                        (pid.to_string(), v)
                    })
                    .collect();
                json!({ "summary": pr.summary(), "manifest": pr.manifest, "pages": pages, "dir": pr.dir.to_string_lossy() })
            }
            "createProfile" => {
                let device = p_str(&p, "device")?;
                let spec = crate::engine::spec_for(self, device).ok_or_else(|| anyhow!("unknown device"))?;
                let name = p.get("name").and_then(|n| n.as_str()).unwrap_or("New Profile");
                let id = self.store.create(name, spec, device)?;
                self.emit("profilesChanged", json!({}));
                json!(id.to_string())
            }
            "deleteProfile" => {
                let id = p_uuid(&p, "profile")?;
                let in_use: Vec<String> = self.devices.iter().filter(|(_, d)| d.profile == Some(id)).map(|(k, _)| k.clone()).collect();
                let spec_ok = self.store.get(&id).is_some();
                if !spec_ok {
                    bail!("unknown profile");
                }
                self.store.delete(&id)?;
                for d in in_use {
                    let next = self.store.for_device(&d, self.devices[&d].spec()).map(|pr| pr.id).next();
                    let next = match next {
                        Some(n) => n,
                        None => self.store.create("Default Profile", self.devices[&d].spec(), &d)?,
                    };
                    self.switch_profile(&d, next, None);
                }
                self.emit("profilesChanged", json!({}));
                json!(true)
            }
            "renameProfile" => {
                let id = p_uuid(&p, "profile")?;
                let pr = self.store.get_mut(&id).ok_or_else(|| anyhow!("unknown profile"))?;
                pr.manifest.name = p_str(&p, "name")?.to_string();
                pr.save_manifest()?;
                self.emit("profilesChanged", json!({}));
                self.emit_state();
                json!(true)
            }
            "duplicateProfile" => {
                let id = self.store.duplicate(&p_uuid(&p, "profile")?, p.get("name").and_then(|n| n.as_str()))?;
                self.emit("profilesChanged", json!({}));
                json!(id.to_string())
            }
            "setProfileApp" => {
                let id = p_uuid(&p, "profile")?;
                let pr = self.store.get_mut(&id).ok_or_else(|| anyhow!("unknown profile"))?;
                pr.manifest.app_identifier = p.get("app").and_then(|a| a.as_str()).filter(|s| !s.is_empty()).map(String::from);
                pr.save_manifest()?;
                self.emit("profilesChanged", json!({}));
                json!(true)
            }
            "selectProfile" => {
                let device = p_str(&p, "device")?.to_string();
                let id = p_uuid(&p, "profile")?;
                if self.store.get(&id).is_none() {
                    bail!("unknown profile");
                }
                let page = p.get("page").and_then(|v| v.as_str()).and_then(|s| s.parse().ok());
                self.settings.device(&device).profile = Some(id.to_string());
                self.settings.save();
                if let Some(d) = self.devices.get_mut(&device) {
                    d.smart_return = None;
                }
                self.switch_profile(&device, id, page);
                json!(true)
            }
            "importProfile" => {
                let path = PathBuf::from(p_str(&p, "path")?);
                let device = p.get("device").and_then(|d| d.as_str()).map(String::from);
                let target = device.as_deref().and_then(|d| self.devices.get(d).map(|dev| (dev.spec(), d.to_string())));
                let ids = self.store.import(&path, target.as_ref().map(|(s, d)| (*s, d.as_str())))?;
                self.emit("profilesChanged", json!({}));
                json!(ids.iter().map(|i| i.to_string()).collect::<Vec<_>>())
            }
            "exportProfile" => {
                self.flush();
                let id = p_uuid(&p, "profile")?;
                let path = PathBuf::from(p_str(&p, "path")?);
                self.store.export(&id, &path)?;
                json!(path.to_string_lossy())
            }
            "addPage" => {
                let id = p_uuid(&p, "profile")?;
                let spec = self.store.get(&id).and_then(|pr| pr.spec()).or_else(|| self.devices.values().find(|d| d.profile == Some(id)).map(|d| d.spec())).ok_or_else(|| anyhow!("unknown profile"))?;
                let folder = p.get("folder").and_then(|f| f.as_bool()).unwrap_or(false);
                let page = self.store.add_page(&id, spec, folder)?;
                self.emit("profileChanged", json!({ "profile": id.to_string() }));
                self.emit_state();
                json!(page.to_string())
            }
            "deletePage" => {
                let id = p_uuid(&p, "profile")?;
                let page = p_uuid(&p, "page")?;
                self.store.remove_page(&id, &page)?;
                let devs: Vec<String> = self.devices.iter().filter(|(_, d)| d.profile == Some(id)).map(|(k, _)| k.clone()).collect();
                for d in devs {
                    let first = self.store.get(&id).and_then(|pr| pr.current_page());
                    if self.devices[&d].page == Some(page) || self.devices[&d].folders.contains(&page) {
                        if let Some(f) = first {
                            self.switch_page(&d, f, true);
                        }
                    }
                }
                self.emit("profileChanged", json!({ "profile": id.to_string() }));
                self.emit_state();
                json!(true)
            }
            "movePage" => {
                let id = p_uuid(&p, "profile")?;
                let page = p_str(&p, "page")?.to_string();
                let index = p.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                let pr = self.store.get_mut(&id).ok_or_else(|| anyhow!("unknown profile"))?;
                let pages = &mut pr.manifest.pages.pages;
                let from = pages.iter().position(|x| x.eq_ignore_ascii_case(&page)).ok_or_else(|| anyhow!("unknown page"))?;
                let item = pages.remove(from);
                pages.insert(index.min(pages.len()), item);
                pr.save_manifest()?;
                self.emit("profileChanged", json!({ "profile": id.to_string() }));
                self.emit_state();
                json!(true)
            }
            "renamePage" => {
                let id = p_uuid(&p, "profile")?;
                let page = p_uuid(&p, "page")?;
                let pr = self.store.get_mut(&id).ok_or_else(|| anyhow!("unknown profile"))?;
                pr.pages.get_mut(&page).ok_or_else(|| anyhow!("unknown page"))?.name = p_str(&p, "name")?.to_string();
                pr.save_page(&page)?;
                self.emit("profileChanged", json!({ "profile": id.to_string() }));
                json!(true)
            }
            "setDefaultPage" => {
                let id = p_uuid(&p, "profile")?;
                let page = p_str(&p, "page")?.to_string();
                let pr = self.store.get_mut(&id).ok_or_else(|| anyhow!("unknown profile"))?;
                pr.manifest.pages.default = page;
                pr.save_manifest()?;
                json!(true)
            }
            "selectPage" => {
                let device = p_str(&p, "device")?.to_string();
                let page = p_uuid(&p, "page")?;
                let profile = self.devices.get(&device).and_then(|d| d.profile).ok_or_else(|| anyhow!("unknown device"))?;
                if !self.store.get(&profile).map(|pr| pr.pages.contains_key(&page)).unwrap_or(false) {
                    bail!("page is not in the device's profile");
                }
                let folder = self.store.get(&profile).map(|pr| pr.is_folder(&page)).unwrap_or(false);
                if folder {
                    // Showing a folder from the editor keeps a way back to its page.
                    let parent = self.store.get(&profile).and_then(|pr| pr.parent_of(&page));
                    if let Some(d) = self.devices.get_mut(&device) {
                        d.folders = parent.into_iter().collect();
                    }
                    if let Some(d) = self.devices.get_mut(&device) {
                        d.page = Some(page);
                    }
                    self.show(&device);
                    self.emit_state();
                } else {
                    self.switch_page(&device, page, true);
                }
                json!(true)
            }
            "pageStep" => {
                let device = p_str(&p, "device")?.to_string();
                self.page_step(&device, p.get("delta").and_then(|d| d.as_i64()).unwrap_or(1) as i32);
                json!(true)
            }
            "closeFolder" => {
                let device = p_str(&p, "device")?.to_string();
                self.close_folder(&device);
                json!(true)
            }
            "getAction" => {
                let ctx = p_str(&p, "context")?;
                let loc = self.locate(ctx).ok_or_else(|| anyhow!("unknown context"))?;
                let a = self.action_at(&loc).cloned();
                json!({ "action": a, "loc": loc_json(&loc), "trigger": self.instances.get(ctx).and_then(|i| i.trigger.clone()) })
            }
            "setAction" => {
                let loc = loc_from(&p)?;
                let uuid = p_str(&p, "uuid")?.to_string();
                let action = self.new_action_from_manifest(&uuid, &loc)?;
                self.put_action(&loc, Some(action.clone()))?;
                json!(action)
            }
            "updateAction" => {
                let loc = loc_from(&p)?;
                let mut action: ActionData = serde_json::from_value(p.get("action").cloned().ok_or_else(|| anyhow!("missing action"))?)?;
                ensure_ids(&mut action);
                let ctx = action.action_id.clone();
                // A title edited by the user replaces whatever the plugin set.
                if let Some(inst) = self.instances.get_mut(&ctx) {
                    inst.title.clear();
                }
                self.put_action(&loc, Some(action.clone()))?;
                self.send_settings_to_plugin(&ctx);
                json!(action)
            }
            "removeAction" => {
                let loc = loc_from(&p)?;
                self.put_action(&loc, None)?;
                json!(true)
            }
            "moveAction" => {
                let from = loc_from(&p)?;
                let to = loc_from(p.get("to").ok_or_else(|| anyhow!("missing `to`"))?)?;
                let copy = p.get("copy").and_then(|c| c.as_bool()).unwrap_or(false);
                let mut a = self.action_at(&from).cloned().ok_or_else(|| anyhow!("nothing at source"))?;
                let displaced = self.action_at(&to).cloned();
                if copy {
                    a = self.clone_action_files(&a, &from, &to)?;
                } else {
                    a = self.move_action_files(a, &from, &to)?;
                    // Moving onto an occupied key swaps the two, as dragging does.
                    let back = match displaced {
                        Some(d) if from.path.is_empty() && to.path.is_empty() => Some(self.move_action_files(d, &to, &from)?),
                        _ => None,
                    };
                    self.put_action(&from, back)?;
                }
                self.put_action(&to, Some(a.clone()))?;
                json!(a)
            }
            "setActionImage" => {
                let loc = loc_from(&p)?;
                let state = p.get("state").and_then(|s| s.as_u64()).unwrap_or(0) as usize;
                let rel = if let Some(path) = p.get("path").and_then(|v| v.as_str()) {
                    let bytes = std::fs::read(path)?;
                    let ext = Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("png").to_lowercase();
                    Some(self.store.get(&loc.profile).ok_or_else(|| anyhow!("unknown profile"))?.store_image(&loc.page, &bytes, &ext)?)
                } else if let Some(data) = p.get("data").and_then(|v| v.as_str()) {
                    let bytes = base64::engine::general_purpose::STANDARD.decode(data)?;
                    Some(self.store.get(&loc.profile).ok_or_else(|| anyhow!("unknown profile"))?.store_image(&loc.page, &bytes, "png")?)
                } else {
                    None
                };
                let encoder = p.get("target").and_then(|t| t.as_str()) == Some("background");
                let a = self.action_at_mut(&loc).ok_or_else(|| anyhow!("no action there"))?;
                if encoder {
                    let e = a.encoder.get_or_insert_with(|| json!({}));
                    e["background"] = json!(rel.clone().unwrap_or_default());
                } else {
                    while a.states.len() <= state {
                        a.states.push(Default::default());
                    }
                    a.states[state].image = rel;
                }
                let ctx = a.action_id.clone();
                if let Some(inst) = self.instances.get_mut(&ctx) {
                    inst.image.clear();
                    inst.dyn_image = None;
                }
                self.flush();
                self.after_edit(&loc);
                json!(true)
            }
            "setActionState" => {
                let ctx = p_str(&p, "context")?.to_string();
                self.set_state(&ctx, p.get("state").and_then(|s| s.as_u64()).unwrap_or(0) as u32);
                json!(true)
            }
            "piInfo" => {
                let ctx = p_str(&p, "context")?;
                let loc = self.locate(ctx).ok_or_else(|| anyhow!("unknown context"))?;
                let a = self.action_at(&loc).cloned().ok_or_else(|| anyhow!("gone"))?;
                let def = self.registry.action(&a.uuid).ok_or_else(|| anyhow!("{} is not installed", a.uuid))?;
                let plugin = self.registry.plugins.get(&def.plugin).ok_or_else(|| anyhow!("plugin missing"))?;
                let pi = def.property_inspector.clone().or_else(|| plugin.property_inspector.clone());
                let device = self.instances.get(ctx).map(|i| i.device.clone()).or_else(|| self.devices.iter().find(|(_, d)| d.profile == Some(loc.profile)).map(|(k, _)| k.clone())).unwrap_or_default();
                let coords = Slot::parse(loc.ctl.name(), &loc.pos).map(|s| s.coordinates());
                let builtin = plugin.builtin;
                let info = crate::engine::info_payload(self, &def.plugin);
                json!({
                    "port": self.sdk_port,
                    "uuid": ctx,
                    "registerEvent": "registerPropertyInspector",
                    "info": info,
                    "actionInfo": { "action": a.uuid, "context": ctx, "device": device, "payload": { "settings": a.settings, "coordinates": coords, "controller": loc.ctl.name(), "state": a.state, "isInMultiAction": !loc.path.is_empty() } },
                    "path": pi,
                    "builtin": builtin,
                    "pluginRunning": builtin || self.running_plugins().contains(&def.plugin),
                    "unsupported": plugin.unsupported,
                })
            }
            "press" => {
                let device = p_str(&p, "device")?.to_string();
                let ctl = p.get("controller").and_then(|c| c.as_str()).unwrap_or("Keypad");
                let slot = Slot::parse(ctl, p_str(&p, "position")?).ok_or_else(|| anyhow!("bad position"))?;
                let down = p.get("down").and_then(|d| d.as_bool());
                if self.devices.get(&device).map(|d| d.asleep).unwrap_or(false) {
                    self.wake(&device);
                    return Ok(json!(true));
                }
                match down {
                    Some(d) => self.key_event(&device, slot, d),
                    None => {
                        self.key_event(&device, slot, true);
                        self.key_event(&device, slot, false);
                    }
                }
                json!(true)
            }
            "dial" => {
                let device = p_str(&p, "device")?.to_string();
                let index = p.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as u8;
                let input = if let Some(t) = p.get("ticks").and_then(|t| t.as_i64()) {
                    crate::device::DeviceInput::DialRotate(index, t as i32)
                } else if p.get("touch").and_then(|t| t.as_bool()).unwrap_or(false) {
                    let w = self.devices.get(&device).and_then(|d| d.spec().lcd).map(|(w, _)| w).unwrap_or(800);
                    let enc = self.devices.get(&device).map(|d| d.spec().encoders).unwrap_or(4).max(1) as u32;
                    crate::device::DeviceInput::Touch { x: (index as u32 * (w / enc) + w / enc / 2) as u16, y: 50, long: p.get("long").and_then(|l| l.as_bool()).unwrap_or(false) }
                } else if p.get("down").and_then(|d| d.as_bool()).unwrap_or(true) {
                    crate::device::DeviceInput::DialDown(index)
                } else {
                    crate::device::DeviceInput::DialUp(index)
                };
                self.tx.send(Msg::Device(crate::device::EngineEvent::Input(device, input))).ok();
                json!(true)
            }
            "setBrightness" => {
                let device = p_str(&p, "device")?.to_string();
                let v = p.get("value").and_then(|v| v.as_u64()).unwrap_or(60).min(100) as u8;
                self.settings.device(&device).brightness = v;
                self.settings.save();
                if let Some(d) = self.devices.get(&device) {
                    if !d.asleep {
                        d.tx.send(crate::device::RenderJob::Brightness(v)).ok();
                    }
                }
                self.emit_state();
                json!(true)
            }
            "setSleep" => {
                let device = p_str(&p, "device")?.to_string();
                self.settings.device(&device).sleep_minutes = p.get("minutes").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                self.settings.save();
                self.emit_state();
                json!(true)
            }
            "sleep" => {
                let device = p_str(&p, "device")?.to_string();
                self.sleep_device(&device);
                json!(true)
            }
            "wake" => {
                let device = p_str(&p, "device")?.to_string();
                self.wake(&device);
                json!(true)
            }
            "renameDevice" => {
                let device = p_str(&p, "device")?.to_string();
                self.settings.device(&device).name = p.get("name").and_then(|n| n.as_str()).filter(|s| !s.is_empty()).map(String::from);
                self.settings.save();
                self.emit_state();
                json!(true)
            }
            "listApplications" => json!(crate::system::list_applications()),
            "listFonts" => json!(crate::render::font_families()),
            "installPlugin" => {
                let uuid = plugins::install(Path::new(p_str(&p, "path")?))?;
                self.stop_plugin(&uuid);
                self.reload_registry();
                self.restart_plugin(&uuid);
                self.emit("pluginsChanged", json!({}));
                json!(uuid)
            }
            "uninstallPlugin" => {
                let uuid = p_str(&p, "uuid")?.to_string();
                self.stop_plugin(&uuid);
                plugins::uninstall(&uuid)?;
                self.reload_registry();
                self.emit("pluginsChanged", json!({}));
                json!(true)
            }
            "restartPlugin" => {
                let uuid = p_str(&p, "uuid")?.to_string();
                self.restart_plugin(&uuid);
                json!(true)
            }
            "pluginLog" => {
                let uuid = p_str(&p, "uuid")?;
                let text = std::fs::read_to_string(crate::paths::logs_dir().join(format!("{uuid}.log"))).unwrap_or_default();
                let tail: String = text.chars().rev().take(20000).collect::<Vec<_>>().into_iter().rev().collect();
                json!(tail)
            }
            "listIconPacks" => json!(list_icon_packs()),
            "installIconPack" => json!(install_icon_pack(Path::new(p_str(&p, "path")?))?),
            "removeIconPack" => {
                let id = p_str(&p, "id")?;
                let dir = crate::paths::icon_packs_dir().join(format!("{id}.sdIconPack"));
                if !dir.starts_with(crate::paths::icon_packs_dir()) || !dir.exists() {
                    bail!("unknown icon pack");
                }
                std::fs::remove_dir_all(dir)?;
                json!(true)
            }
            "backup" => {
                self.flush();
                let path = p.get("path").and_then(|v| v.as_str()).map(PathBuf::from).unwrap_or_else(|| crate::paths::backups_dir().join(format!("backup-{}.streamDeckProfilesBackup", chrono::Local::now().format("%Y%m%d-%H%M%S"))));
                self.store.backup(&path)?;
                json!(path.to_string_lossy())
            }
            "listBackups" => {
                let mut list: Vec<Value> = std::fs::read_dir(crate::paths::backups_dir())
                    .map(|rd| {
                        rd.flatten()
                            .filter_map(|e| {
                                let m = e.metadata().ok()?;
                                let modified = m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
                                Some(json!({ "path": e.path().to_string_lossy(), "name": e.file_name().to_string_lossy(), "size": m.len(), "modified": modified }))
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                list.sort_by_key(|v| std::cmp::Reverse(v["modified"].as_u64().unwrap_or(0)));
                json!(list)
            }
            "restoreBackup" => {
                self.flush();
                let path = PathBuf::from(p_str(&p, "path")?);
                let devs: Vec<String> = self.devices.keys().cloned().collect();
                for d in &devs {
                    let ctxs: Vec<String> = self.devices[d].visible.values().cloned().collect();
                    for c in ctxs {
                        self.disappear(&c);
                    }
                    if let Some(dev) = self.devices.get_mut(d) {
                        dev.visible.clear();
                    }
                }
                self.store.restore(&path)?;
                for d in devs {
                    let remembered = self.settings.devices.get(&d).and_then(|s| s.profile.clone());
                    let pr = self.pick_profile_pub(&d, remembered.as_deref());
                    self.switch_profile(&d, pr, None);
                }
                self.emit("profilesChanged", json!({}));
                json!(true)
            }
            "getSettings" => serde_json::to_value(&self.settings)?,
            "setSettings" => {
                let mut current = serde_json::to_value(&self.settings)?;
                if let (Some(c), Some(n)) = (current.as_object_mut(), p.as_object()) {
                    for (k, v) in n {
                        if k != "devices" && k != "virtualDecks" {
                            c.insert(k.clone(), v.clone());
                        }
                    }
                }
                let lang_before = self.settings.language.clone();
                self.settings = serde_json::from_value(current)?;
                self.settings.save();
                if self.settings.language != lang_before {
                    self.reload_registry();
                }
                self.reappear_all();
                json!(true)
            }
            "addVirtualDeck" => {
                let columns = p.get("columns").and_then(|v| v.as_u64()).unwrap_or(8).clamp(1, 16) as u8;
                let rows = p.get("rows").and_then(|v| v.as_u64()).unwrap_or(4).clamp(1, 16) as u8;
                let n = self.settings.virtual_decks.len() + 1;
                let id = format!("virtual-{}", Uuid::new_v4().simple().to_string().get(..8).unwrap_or("x"));
                let name = p.get("name").and_then(|v| v.as_str()).map(String::from).unwrap_or_else(|| format!("Virtual Stream Deck {n}"));
                self.settings.virtual_decks.push(crate::settings::VirtualDeck { id: id.clone(), name: name.clone(), columns, rows });
                self.settings.save();
                crate::spawn_virtual(self, &id, &name, columns, rows);
                json!(id)
            }
            "removeVirtualDeck" => {
                let id = p_str(&p, "id")?.to_string();
                self.settings.virtual_decks.retain(|v| v.id != id);
                self.settings.save();
                self.tx.send(Msg::Device(crate::device::EngineEvent::Disconnected(id))).ok();
                json!(true)
            }
            "deepLink" => {
                self.deep_link(p_str(&p, "url")?)?;
                json!(true)
            }
            "reload" => {
                self.flush();
                self.store.reload()?;
                self.reload_registry();
                self.reappear_all();
                self.emit("profilesChanged", json!({}));
                json!(true)
            }
            other => bail!("unknown method {other}"),
        };
        Ok(r)
    }

    pub fn pick_profile_pub(&mut self, dev: &str, remembered: Option<&str>) -> Uuid {
        let spec = self.devices[dev].spec();
        if let Some(p) = remembered.and_then(|r| r.parse::<Uuid>().ok()).filter(|p| self.store.get(p).is_some()) {
            return p;
        }
        if let Some(p) = self.store.for_device(dev, spec).map(|p| p.id).next() {
            return p;
        }
        self.store.create("Default Profile", spec, dev).expect("create profile")
    }

    fn reload_registry(&mut self) {
        self.registry = plugins::Registry::load(&self.settings.language);
        self.start_all_plugins();
    }

    fn new_action_from_manifest(&self, uuid: &str, loc: &Loc) -> Result<ActionData> {
        let def = self.registry.action(uuid).ok_or_else(|| anyhow!("unknown action {uuid}"))?;
        if !def.controllers.iter().any(|c| c == loc.ctl.name()) && loc.path.is_empty() {
            bail!("{} cannot go on a {}", def.name, loc.ctl.name().to_lowercase());
        }
        let plugin = self.registry.plugins.get(&def.plugin);
        let mut a = store::new_action(uuid, &def.name, json!({}));
        a.states = def
            .states
            .iter()
            .map(|_| crate::model::StateData::default())
            .collect();
        if a.states.is_empty() {
            a.states.push(Default::default());
        }
        a.plugin = plugin.map(|p| crate::model::PluginRef { name: p.name.clone(), uuid: p.uuid.clone(), version: p.version.clone(), ..Default::default() });
        // Containers start with the routines they need.
        let routines = match uuid {
            crate::builtins::MULTI | crate::builtins::RANDOM | crate::builtins::ACTION_WHEEL | crate::builtins::DIAL_STACK => 1,
            crate::builtins::MULTI_SWITCH => 2,
            crate::builtins::KEY_LOGIC => 3,
            crate::builtins::ACTION_TRIGGER => 4,
            _ => 0,
        };
        if routines > 0 {
            a.actions = Some((0..routines).map(|_| Default::default()).collect());
        }
        if uuid == crate::builtins::FOLDER {
            // Creating the folder page here keeps the action and its page together.
            a.settings = json!({ "ProfileUUID": "" });
        }
        Ok(a)
    }

    /// Writes (or clears) the action at `loc` and refreshes devices.
    fn put_action(&mut self, loc: &Loc, action: Option<ActionData>) -> Result<()> {
        let mut action = action;
        if let Some(a) = action.as_mut() {
            if a.uuid == crate::builtins::FOLDER && a.setting_str("ProfileUUID").parse::<Uuid>().map(|id| !self.store.get(&loc.profile).map(|p| p.pages.contains_key(&id)).unwrap_or(false)).unwrap_or(true) {
                let spec = self.store.get(&loc.profile).and_then(|p| p.spec()).or_else(|| self.devices.values().find(|d| d.profile == Some(loc.profile)).map(|d| d.spec())).unwrap_or(&crate::catalog::SPECS[1]);
                let folder = self.store.add_page(&loc.profile, spec, true)?;
                a.settings = json!({ "ProfileUUID": folder.to_string() });
            }
        }
        if loc.path.is_empty() {
            let pr = self.store.get_mut(&loc.profile).ok_or_else(|| anyhow!("unknown profile"))?;
            let page = pr.pages.get_mut(&loc.page).ok_or_else(|| anyhow!("unknown page"))?;
            let c = page.controller_mut(loc.ctl.name());
            match action {
                Some(a) => {
                    c.actions.insert(loc.pos.clone(), a);
                }
                None => {
                    c.actions.remove(&loc.pos);
                }
            }
        } else {
            // Child of a container: path's last step names the slot in a routine.
            let (r, i) = *loc.path.last().unwrap();
            let mut parent = loc.clone();
            parent.path.pop();
            let p = self.action_at_mut(&parent).ok_or_else(|| anyhow!("no container there"))?;
            let routines = p.actions.get_or_insert_with(Vec::new);
            while routines.len() <= r {
                routines.push(Default::default());
            }
            let list = &mut routines[r].actions;
            match action {
                Some(a) if i < list.len() => list[i] = a,
                Some(a) => list.push(a),
                None if i < list.len() => {
                    list.remove(i);
                }
                None => {}
            }
        }
        self.mark_dirty(loc.profile, loc.page);
        self.flush();
        self.after_edit(loc);
        Ok(())
    }

    fn after_edit(&mut self, loc: &Loc) {
        // The edited key is torn down and shown again so plugins get a fresh
        // willAppear with the new settings.
        let devs: Vec<String> = self.devices.iter().filter(|(_, d)| d.profile == Some(loc.profile) && d.page == Some(loc.page)).map(|(k, _)| k.clone()).collect();
        for d in devs {
            if let Some(slot) = Slot::parse(loc.ctl.name(), &loc.pos) {
                if let Some(ctx) = self.devices.get_mut(&d).and_then(|dev| dev.visible.remove(&slot)) {
                    self.disappear(&ctx);
                }
            }
            if let Some(dev) = self.devices.get_mut(&d) {
                dev.last_visual.clear();
            }
            self.show(&d);
        }
        self.emit("profileChanged", json!({ "profile": loc.profile.to_string(), "page": loc.page.to_string() }));
    }

    fn send_settings_to_plugin(&mut self, ctx: &str) {
        let Some(a) = self.ctx_action(ctx).cloned() else { return };
        let Some(def) = self.registry.action(&a.uuid) else { return };
        let plugin = def.plugin.clone();
        let device = self.instances.get(ctx).map(|i| i.device.clone()).unwrap_or_default();
        self.send_plugin(&plugin, json!({ "event": "didReceiveSettings", "action": a.uuid, "context": ctx, "device": device, "payload": { "settings": a.settings, "state": a.state, "isInMultiAction": false } }));
    }

    fn move_action_files(&mut self, mut a: ActionData, from: &Loc, to: &Loc) -> Result<ActionData> {
        if from.profile == to.profile && from.page == to.page {
            return Ok(a);
        }
        self.copy_images(&mut a, from, to)?;
        Ok(a)
    }

    fn clone_action_files(&mut self, a: &ActionData, from: &Loc, to: &Loc) -> Result<ActionData> {
        let mut a = a.clone();
        fn renew(a: &mut ActionData) {
            a.action_id = Uuid::new_v4().to_string();
            if let Some(rs) = a.actions.as_mut() {
                for r in rs {
                    for c in &mut r.actions {
                        renew(c);
                    }
                }
            }
        }
        renew(&mut a);
        if a.uuid == crate::builtins::FOLDER {
            // A copied folder gets its own (empty) folder page.
            a.settings = json!({ "ProfileUUID": "" });
        }
        self.copy_images(&mut a, from, to)?;
        Ok(a)
    }

    /// Custom images live in the page's directory, so moving a key to
    /// another page or profile carries its images along.
    fn copy_images(&mut self, a: &mut ActionData, from: &Loc, to: &Loc) -> Result<()> {
        let src = self.store.get(&from.profile).ok_or_else(|| anyhow!("unknown profile"))?.clone();
        let dst = self.store.get(&to.profile).ok_or_else(|| anyhow!("unknown profile"))?.clone();
        for st in &mut a.states {
            if let Some(rel) = st.image.clone().filter(|r| !r.starts_with("data:")) {
                if let Some(path) = src.resolve_image(&from.page, &rel) {
                    let bytes = std::fs::read(&path)?;
                    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("png").to_string();
                    st.image = Some(dst.store_image(&to.page, &bytes, &ext)?);
                }
            }
        }
        Ok(())
    }
}

fn ensure_ids(a: &mut ActionData) {
    if a.action_id.is_empty() {
        a.action_id = Uuid::new_v4().to_string();
    }
    if let Some(rs) = a.actions.as_mut() {
        for r in rs {
            for c in &mut r.actions {
                ensure_ids(c);
            }
        }
    }
}

fn loc_json(l: &Loc) -> Value {
    json!({ "profile": l.profile.to_string(), "page": l.page.to_string(), "controller": l.ctl.name(), "position": l.pos, "path": l.path.iter().map(|(a, b)| json!([a, b])).collect::<Vec<_>>() })
}

pub fn list_icon_packs() -> Vec<Value> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(crate::paths::icon_packs_dir()) else { return out };
    for e in rd.flatten() {
        let dir = e.path();
        if dir.extension().and_then(|x| x.to_str()) != Some("sdIconPack") {
            continue;
        }
        let m: Value = std::fs::read(dir.join("manifest.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(json!({}));
        let icons: Vec<Value> = std::fs::read(dir.join("icons.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<Vec<Value>>(&b).ok())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|i| {
                let path = i.get("path").and_then(|p| p.as_str())?;
                Some(json!({ "path": dir.join("icons").join(path).to_string_lossy(), "name": i.get("name"), "tags": i.get("tags") }))
            })
            .collect();
        out.push(json!({
            "id": dir.file_stem().map(|s| s.to_string_lossy().to_string()),
            "name": m.get("Name"),
            "author": m.get("Author"),
            "version": m.get("Version"),
            "icon": m.get("Icon").and_then(|i| i.as_str()).map(|i| dir.join(i).to_string_lossy().to_string()),
            "icons": icons,
        }));
    }
    out
}

/// `.streamDeckIconPack` archives hold `<id>.sdIconPack/` with a manifest,
/// `icons.json` and an `icons/` folder.
pub fn install_icon_pack(archive: &Path) -> Result<String> {
    let root = crate::paths::icon_packs_dir();
    std::fs::create_dir_all(&root)?;
    let staging = store::tempdir_in(&root)?;
    let res = (|| -> Result<String> {
        if archive.is_dir() {
            store::copy_dir(archive, &staging.join(archive.file_name().unwrap()))?;
        } else {
            store::unzip(archive, &staging)?;
        }
        let dir = std::fs::read_dir(&staging)?
            .flatten()
            .map(|e| e.path())
            .find(|p| p.extension().and_then(|e| e.to_str()) == Some("sdIconPack"))
            .ok_or_else(|| anyhow!("archive has no .sdIconPack folder"))?;
        let name = dir.file_name().unwrap().to_owned();
        let dest = root.join(&name);
        if dest.exists() {
            std::fs::remove_dir_all(&dest)?;
        }
        std::fs::rename(&dir, &dest)?;
        Ok(Path::new(&name).file_stem().unwrap().to_string_lossy().to_string())
    })();
    std::fs::remove_dir_all(&staging).ok();
    res
}
