//! streamdeckd: the Stream Deck service.
//!
//! ```text
//! streamdeckd                 run the service (systemd user unit)
//! streamdeckd status          print devices and profiles of the running service
//! streamdeckd ctl METHOD [JSON]  call any control API method
//! streamdeckd open FILE|URL   import a profile, plugin or icon pack, or pass a streamdeck:// link
//! ```

mod audio;
mod builtins;
mod catalog;
mod control;
mod device;
mod engine;
mod input;
mod model;
mod paths;
mod plugins;
mod render;
mod sdk;
mod settings;
mod store;
mod system;

use anyhow::{anyhow, bail, Result};
use device::DeviceManager;
use engine::{Engine, Msg};
use serde_json::{json, Value};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{broadcast, mpsc};

static DEVICES: OnceLock<Arc<DeviceManager>> = OnceLock::new();

/// Starts a Virtual Stream Deck of any size. Specs are 'static for the
/// catalog's sake; the handful a user creates are leaked deliberately.
pub fn spawn_virtual(_e: &Engine, id: &str, name: &str, columns: u8, rows: u8) {
    let Some(dm) = DEVICES.get() else { return };
    let spec: &'static catalog::DeviceSpec = Box::leak(Box::new(catalog::DeviceSpec {
        id: "virtual",
        name: "Virtual Stream Deck",
        columns,
        rows,
        encoders: 0,
        touch_points: 0,
        lcd: None,
        key_px: 72,
        visual: true,
        sdk_type: 11,
        model_code: "VSD",
    }));
    dm.spawn_virtual(id, name, spec);
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    match args.get(1).map(String::as_str) {
        None | Some("run") | Some("--foreground") => rt.block_on(run()),
        Some("--version") | Some("version") => {
            println!("streamdeckd {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some("status") => rt.block_on(async {
            let state = call("getState", json!({})).await?;
            let devices = state["devices"].as_array().cloned().unwrap_or_default();
            if devices.is_empty() {
                println!("No Stream Deck connected.");
            }
            for d in devices {
                println!(
                    "{} ({}{}): profile \"{}\", page {}/{}, brightness {}%{}",
                    d["name"].as_str().unwrap_or("?"),
                    d["modelName"].as_str().unwrap_or("?"),
                    if d["virtual"].as_bool().unwrap_or(false) { ", virtual" } else { "" },
                    d["profileName"].as_str().unwrap_or("?"),
                    d["pageIndex"].as_u64().map(|i| i + 1).unwrap_or(1),
                    d["pageCount"].as_u64().unwrap_or(1),
                    d["brightness"],
                    if d["asleep"].as_bool().unwrap_or(false) { ", asleep" } else { "" }
                );
            }
            Ok(())
        }),
        Some("ctl") => rt.block_on(async {
            let method = args.get(2).ok_or_else(|| anyhow!("usage: streamdeckd ctl METHOD [JSON]"))?;
            let params: Value = match args.get(3) {
                Some(s) => serde_json::from_str(s)?,
                None => json!({}),
            };
            let r = call(method, params).await?;
            println!("{}", serde_json::to_string_pretty(&r)?);
            Ok(())
        }),
        Some("open") => rt.block_on(async {
            let target = args.get(2).ok_or_else(|| anyhow!("usage: streamdeckd open FILE|URL"))?;
            let r = if target.starts_with("streamdeck://") {
                call("deepLink", json!({ "url": target })).await?
            } else {
                let path = std::fs::canonicalize(target)?;
                let p = path.to_string_lossy().to_string();
                if p.ends_with(".streamDeckPlugin") || p.ends_with(".sdPlugin") {
                    call("installPlugin", json!({ "path": p })).await?
                } else if p.ends_with(".streamDeckIconPack") || p.ends_with(".sdIconPack") {
                    call("installIconPack", json!({ "path": p })).await?
                } else if p.ends_with(".streamDeckProfilesBackup") {
                    call("restoreBackup", json!({ "path": p })).await?
                } else {
                    let device = call("getState", json!({})).await?["devices"][0]["id"].as_str().map(String::from);
                    call("importProfile", json!({ "path": p, "device": device })).await?
                }
            };
            println!("{}", serde_json::to_string_pretty(&r)?);
            Ok(())
        }),
        Some(other) => bail!("unknown command {other}; try `status`, `ctl`, or `open`"),
    }
}

async fn call(method: &str, params: Value) -> Result<Value> {
    let path = paths::control_socket();
    let stream = tokio::net::UnixStream::connect(&path).await.map_err(|e| anyhow!("streamdeckd is not running ({}: {e})", path.display()))?;
    let (r, mut w) = stream.into_split();
    w.write_all(format!("{}\n", json!({ "id": 1, "method": method, "params": params })).as_bytes()).await?;
    let mut lines = BufReader::new(r).lines();
    while let Some(line) = lines.next_line().await? {
        let v: Value = serde_json::from_str(&line)?;
        if v.get("id") == Some(&json!(1)) {
            if let Some(e) = v.get("error") {
                bail!("{}", e.as_str().unwrap_or("error"));
            }
            return Ok(v["result"].clone());
        }
    }
    bail!("no reply")
}

async fn run() -> Result<()> {
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "streamdeckd=info,plugin=info".into())).init();
    for d in [paths::data_dir(), paths::profiles_dir(), paths::plugins_dir(), paths::runtime_dir()] {
        std::fs::create_dir_all(&d)?;
    }
    let (tx, rx) = mpsc::unbounded_channel::<Msg>();

    let initial = settings::Settings::load();
    let (sdk_tx, mut sdk_rx) = mpsc::unbounded_channel();
    let port = sdk::serve(initial.sdk_port, sdk_tx).await?;
    tracing::info!("SDK WebSocket on 127.0.0.1:{port}");
    {
        let tx = tx.clone();
        tokio::spawn(async move {
            while let Some(e) = sdk_rx.recv().await {
                if tx.send(Msg::Sdk(e)).is_err() {
                    break;
                }
            }
        });
    }

    let (previews, _) = broadcast::channel(512);
    let frames: device::FrameStore = Default::default();
    control::serve(paths::control_socket(), tx.clone(), previews.clone(), frames.clone()).await?;
    control::spawn_frame_files(&previews);

    let engine = Engine::new(tx.clone(), port)?;
    let vsds = engine.settings.virtual_decks.clone();

    let (dev_tx, mut dev_rx) = mpsc::unbounded_channel();
    let dm = Arc::new(DeviceManager { engine: dev_tx, previews: previews.clone(), frames });
    DEVICES.set(dm.clone()).ok();
    {
        let tx = tx.clone();
        tokio::spawn(async move {
            while let Some(e) = dev_rx.recv().await {
                if tx.send(Msg::Device(e)).is_err() {
                    break;
                }
            }
        });
    }
    for v in &vsds {
        spawn_virtual(&engine, &v.id, &v.name, v.columns, v.rows);
    }
    // Development aid: simulate hardware models without owning one,
    // e.g. STREAMDECK_FAKE=plus,neo
    if let Ok(list) = std::env::var("STREAMDECK_FAKE") {
        for id in list.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            match catalog::spec(id) {
                Some(spec) => dm.spawn_virtual(&format!("fake-{id}"), &format!("Simulated {}", spec.name), spec),
                None => tracing::warn!("STREAMDECK_FAKE: unknown model {id}"),
            }
        }
    }
    if std::env::var("STREAMDECK_NO_HARDWARE").is_err() {
        dm.spawn_hardware_watch();
    }

    let (hypr_tx, mut hypr_rx) = mpsc::unbounded_channel();
    system::watch_active_window(hypr_tx);
    {
        let tx = tx.clone();
        tokio::spawn(async move {
            while let Some(e) = hypr_rx.recv().await {
                tx.send(Msg::Hypr(e)).ok();
            }
        });
    }
    let (sleep_tx, mut sleep_rx) = mpsc::unbounded_channel();
    system::watch_sleep(sleep_tx);
    {
        let tx = tx.clone();
        tokio::spawn(async move {
            while let Some(going) = sleep_rx.recv().await {
                tx.send(Msg::Sleep(going)).ok();
            }
        });
    }
    let (audio_tx, mut audio_rx) = mpsc::unbounded_channel();
    system::watch_audio(audio_tx);
    {
        let tx = tx.clone();
        tokio::spawn(async move {
            while audio_rx.recv().await.is_some() {
                // One mute arrives as a burst of events; read the state once.
                tokio::time::sleep(Duration::from_millis(50)).await;
                while audio_rx.try_recv().is_ok() {}
                if tx.send(Msg::Exec(Box::new(|e: &mut Engine| e.refresh_mic()))).is_err() {
                    break;
                }
            }
        });
    }
    {
        let tx = tx.clone();
        tokio::spawn(async move {
            let mut t = tokio::time::interval(Duration::from_millis(1000));
            loop {
                t.tick().await;
                if tx.send(Msg::Tick).is_err() {
                    break;
                }
            }
        });
    }
    {
        let tx = tx.clone();
        tokio::spawn(async move {
            let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = term.recv() => {}
            }
            tracing::info!("shutting down");
            tx.send(Msg::Shutdown).ok();
        });
    }
    if let Err(e) = input::available() {
        tracing::warn!("{e}");
    }
    engine.run(rx).await;
    std::fs::remove_file(paths::control_socket()).ok();
    Ok(())
}
