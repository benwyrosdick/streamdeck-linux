//! Plugin manifests and plugin processes.
//!
//! Built-in actions are described by manifests in `resources/builtin` (they
//! have no process; the engine implements them). Installed plugins live in
//! `~/.local/share/streamdeck-linux/Plugins/<uuid>.sdPlugin` and are started
//! the way the original app starts them:
//!
//! ```text
//! <program> -port <p> -pluginUUID <uuid> -registerEvent registerPlugin -info <json>
//! ```

use crate::paths;
use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::process::Child;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionDef {
    pub uuid: String,
    pub plugin: String,
    pub name: String,
    pub tooltip: String,
    pub icon: Option<String>,
    pub property_inspector: Option<String>,
    pub states: Vec<Value>,
    pub controllers: Vec<String>,
    pub supported_in_multi_actions: bool,
    pub supported_in_key_logic: bool,
    pub visible: bool,
    pub disable_automatic_states: bool,
    pub user_title_enabled: bool,
    pub encoder: Option<Value>,
    pub os_supported: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plugin {
    pub uuid: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub url: String,
    pub category: String,
    pub category_icon: Option<String>,
    pub icon: Option<String>,
    pub dir: PathBuf,
    pub builtin: bool,
    pub sdk_version: i64,
    pub property_inspector: Option<String>,
    pub actions: Vec<ActionDef>,
    /// Why this plugin cannot run here, if it cannot.
    pub unsupported: Option<String>,
    #[serde(skip)]
    pub manifest: Value,
}

fn s(v: &Value, k: &str) -> String {
    v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string()
}

/// Image paths in manifests omit the extension (`Images/icon` means
/// `icon.png`, `icon@2x.png` or `icon.svg`); clients get the real file.
fn opt_path(dir: &Path, v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(|x| x.as_str()).filter(|x| !x.is_empty()).map(|p| crate::render::resolve_plugin_image(dir, p).to_string_lossy().to_string())
}

/// Reads the language file next to the manifest (`de.json`, `en.json`, …).
fn localization(dir: &Path, lang: &str) -> Value {
    for l in [lang, lang.split(['-', '_']).next().unwrap_or(lang), "en"] {
        if let Ok(b) = std::fs::read(dir.join(format!("{l}.json"))) {
            if let Ok(v) = serde_json::from_slice::<Value>(&b) {
                return v.get("Localization").cloned().unwrap_or(v);
            }
        }
    }
    Value::Null
}

pub fn load_plugin(dir: &Path, builtin: bool, lang: &str) -> Result<Plugin> {
    let manifest: Value = serde_json::from_slice(&std::fs::read(dir.join("manifest.json")).context("manifest.json")?).context("manifest.json is not valid JSON")?;
    let uuid = {
        let u = s(&manifest, "UUID");
        if u.is_empty() {
            dir.file_name().and_then(|n| n.to_str()).unwrap_or("").trim_end_matches(".sdPlugin").to_string()
        } else {
            u
        }
    };
    let l10n = localization(dir, lang);
    let pick = |key: &str, fallback: String| l10n.get(key).and_then(|v| v.as_str()).map(String::from).unwrap_or(fallback);
    let mut actions = Vec::new();
    for a in manifest.get("Actions").and_then(|a| a.as_array()).cloned().unwrap_or_default() {
        let auuid = s(&a, "UUID");
        let al = l10n.get(&auuid).cloned().unwrap_or(Value::Null);
        let controllers: Vec<String> = a.get("Controllers").and_then(|c| c.as_array()).map(|c| c.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_else(|| vec!["Keypad".into()]);
        let mut states: Vec<Value> = a.get("States").and_then(|s| s.as_array()).cloned().unwrap_or_default();
        if let Some(lstates) = al.get("States").and_then(|s| s.as_array()) {
            for (st, ls) in states.iter_mut().zip(lstates) {
                if let (Some(o), Some(lo)) = (st.as_object_mut(), ls.as_object()) {
                    for (k, v) in lo {
                        o.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        if states.is_empty() {
            states.push(json!({}));
        }
        let os_ok = a.get("OS").and_then(|o| o.as_array()).map(|list| list.iter().any(|x| matches!(x.as_str(), Some("linux") | Some("Linux")))).unwrap_or(true);
        actions.push(ActionDef {
            plugin: uuid.clone(),
            name: al.get("Name").and_then(|v| v.as_str()).map(String::from).unwrap_or_else(|| s(&a, "Name")),
            tooltip: al.get("Tooltip").and_then(|v| v.as_str()).map(String::from).unwrap_or_else(|| s(&a, "Tooltip")),
            icon: opt_path(dir, &a, "Icon"),
            property_inspector: opt_path(dir, &a, "PropertyInspectorPath"),
            controllers,
            supported_in_multi_actions: a.get("SupportedInMultiActions").and_then(|v| v.as_bool()).unwrap_or(true),
            supported_in_key_logic: a.get("SupportedInKeyLogicActions").and_then(|v| v.as_bool()).unwrap_or(true),
            visible: a.get("VisibleInActionsList").and_then(|v| v.as_bool()).unwrap_or(true),
            disable_automatic_states: a.get("DisableAutomaticStates").and_then(|v| v.as_bool()).unwrap_or(false),
            user_title_enabled: a.get("UserTitleEnabled").and_then(|v| v.as_bool()).unwrap_or(true),
            encoder: a.get("Encoder").cloned(),
            os_supported: os_ok,
            states,
            uuid: auuid,
        });
    }
    let sdk_version = manifest.get("SDKVersion").and_then(|v| v.as_i64()).unwrap_or(2);
    let mut p = Plugin {
        name: pick("Name", s(&manifest, "Name")),
        version: s(&manifest, "Version"),
        author: s(&manifest, "Author"),
        description: pick("Description", s(&manifest, "Description")),
        url: s(&manifest, "URL"),
        category: pick("Category", s(&manifest, "Category")),
        category_icon: opt_path(dir, &manifest, "CategoryIcon"),
        icon: opt_path(dir, &manifest, "Icon"),
        dir: dir.to_path_buf(),
        builtin,
        sdk_version,
        property_inspector: opt_path(dir, &manifest, "PropertyInspectorPath"),
        actions,
        unsupported: None,
        manifest,
        uuid,
    };
    if !builtin {
        p.unsupported = launch_plan(&p).err().map(|e| e.to_string());
    }
    Ok(p)
}

pub struct Registry {
    pub plugins: BTreeMap<String, Plugin>,
}

impl Registry {
    pub fn load(lang: &str) -> Registry {
        let mut plugins = BTreeMap::new();
        let builtin_dir = paths::resources_dir().join("builtin");
        for (dir, builtin) in [(builtin_dir, true), (paths::plugins_dir(), false)] {
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().and_then(|e| e.to_str()) != Some("sdPlugin") {
                    continue;
                }
                match load_plugin(&p, builtin, lang) {
                    Ok(plugin) => {
                        plugins.insert(plugin.uuid.clone(), plugin);
                    }
                    Err(e) => tracing::warn!("plugin {}: {e:#}", p.display()),
                }
            }
        }
        Registry { plugins }
    }

    pub fn action(&self, uuid: &str) -> Option<&ActionDef> {
        // Action UUIDs start with their plugin UUID by convention, which makes
        // the lookup cheap; fall back to a scan for plugins that break it.
        let mut prefix = uuid;
        while let Some(i) = prefix.rfind('.') {
            prefix = &prefix[..i];
            if let Some(p) = self.plugins.get(prefix) {
                if let Some(a) = p.actions.iter().find(|a| a.uuid == uuid) {
                    return Some(a);
                }
            }
        }
        self.plugins.values().flat_map(|p| p.actions.iter()).find(|a| a.uuid == uuid)
    }

    pub fn plugin_for_action(&self, uuid: &str) -> Option<&Plugin> {
        let a = self.action(uuid)?;
        self.plugins.get(&a.plugin)
    }
}

#[derive(Debug, Clone)]
pub struct LaunchPlan {
    pub program: String,
    pub args: Vec<String>,
    pub html: bool,
}

/// Decides how to run a plugin on Linux. Plugins built with the official SDK
/// are Node.js programs and run unchanged; a `CodePathLin` (an extension this
/// port honours) can point at a native Linux binary; HTML plugins run in the
/// editor's web engine; macOS and Windows binaries cannot run.
pub fn launch_plan(p: &Plugin) -> Result<LaunchPlan> {
    let m = &p.manifest;
    let lin = s(m, "CodePathLin");
    let generic = s(m, "CodePath");
    let node_declared = m.get("Nodejs").is_some();
    let candidates = [lin.clone(), generic.clone()];
    for code in candidates.iter().filter(|c| !c.is_empty()) {
        let path = p.dir.join(code);
        let lower = code.to_lowercase();
        if lower.ends_with(".html") || lower.ends_with(".htm") {
            return Ok(LaunchPlan { program: "html".into(), args: vec![path.to_string_lossy().to_string()], html: true });
        }
        if node_declared || lower.ends_with(".js") || lower.ends_with(".mjs") || lower.ends_with(".cjs") {
            let node = node_binary().ok_or_else(|| anyhow!("needs Node.js, which was not found on PATH"))?;
            let mut args = Vec::new();
            if let Some(dbg) = m.get("Nodejs").and_then(|n| n.get("Debug")).and_then(|d| d.as_str()) {
                if dbg == "enabled" || dbg.starts_with("--inspect") {
                    args.push(if dbg == "enabled" { "--inspect=127.0.0.1:0".into() } else { dbg.to_string() });
                }
            }
            args.push(path.to_string_lossy().to_string());
            return Ok(LaunchPlan { program: node, args, html: false });
        }
        if code == &lin || is_linux_executable(&path) {
            return Ok(LaunchPlan { program: path.to_string_lossy().to_string(), args: vec![], html: false });
        }
    }
    let mac = s(m, "CodePathMac");
    let win = s(m, "CodePathWin");
    if !win.is_empty() || !mac.is_empty() || !generic.is_empty() {
        bail!("this plugin ships only a {} binary and has no Node.js or Linux build", if !win.is_empty() && !mac.is_empty() { "macOS and Windows" } else if !win.is_empty() { "Windows" } else { "macOS" });
    }
    bail!("manifest has no CodePath")
}

fn is_linux_executable(p: &Path) -> bool {
    let Ok(bytes) = std::fs::read(p).map(|b| b.into_iter().take(4).collect::<Vec<u8>>()) else { return false };
    bytes.starts_with(b"\x7fELF") || bytes.starts_with(b"#!")
}

pub fn node_binary() -> Option<String> {
    if let Ok(n) = std::env::var("STREAMDECK_NODE") {
        return Some(n);
    }
    std::env::var_os("PATH").and_then(|paths| std::env::split_paths(&paths).map(|p| p.join("node")).find(|p| p.is_file())).map(|p| p.to_string_lossy().to_string())
}

/// The editor binary doubles as the HTML plugin host.
pub fn gui_binary() -> String {
    if let Ok(exe) = std::env::current_exe() {
        for c in [exe.with_file_name("streamdeck"), exe.parent().map(|p| p.join("../../../app/build/streamdeck")).unwrap_or_default()] {
            if c.is_file() {
                return c.to_string_lossy().to_string();
            }
        }
    }
    "streamdeck".into()
}

pub struct RunningPlugin {
    pub child: Child,
    pub started: Instant,
    pub restarts: u32,
}

pub fn spawn(plugin: &Plugin, port: u16, info: &Value) -> Result<Child> {
    let plan = launch_plan(plugin)?;
    let (program, mut args) = if plan.html { (gui_binary(), vec!["plugin-host".into(), plan.args[0].clone()]) } else { (plan.program, plan.args) };
    args.extend(["-port".into(), port.to_string(), "-pluginUUID".into(), plugin.uuid.clone(), "-registerEvent".into(), "registerPlugin".into(), "-info".into(), info.to_string()]);
    let logs = paths::logs_dir();
    std::fs::create_dir_all(&logs).ok();
    let log = std::fs::OpenOptions::new().create(true).append(true).open(logs.join(format!("{}.log", plugin.uuid)))?;
    let log2 = log.try_clone()?;
    let data = paths::plugin_data_dir(&plugin.uuid);
    std::fs::create_dir_all(&data).ok();
    let mut cmd = tokio::process::Command::new(&program);
    cmd.args(&args)
        .current_dir(&plugin.dir)
        .stdin(Stdio::null())
        .stdout(log)
        .stderr(log2)
        .env("STREAMDECK_PLUGIN_DATA", &data)
        .kill_on_drop(true);
    // Plugins never outlive the daemon, even if it is killed.
    unsafe {
        cmd.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            Ok(())
        });
    }
    let child = cmd.spawn().map_err(|e| anyhow!("could not start {program}: {e}"))?;
    tracing::info!("started plugin {} ({program})", plugin.uuid);
    Ok(child)
}

/// Installs a `.streamDeckPlugin` archive (a zip holding `<uuid>.sdPlugin/`).
pub fn install(archive: &Path) -> Result<String> {
    let dest_root = paths::plugins_dir();
    std::fs::create_dir_all(&dest_root)?;
    let staging = crate::store::tempdir_in(&dest_root)?;
    let res = (|| -> Result<String> {
        if archive.is_dir() {
            crate::store::copy_dir(archive, &staging.join(archive.file_name().unwrap()))?;
        } else {
            crate::store::unzip(archive, &staging)?;
        }
        let dir = std::fs::read_dir(&staging)?
            .flatten()
            .map(|e| e.path())
            .find(|p| p.extension().and_then(|e| e.to_str()) == Some("sdPlugin") && p.join("manifest.json").exists())
            .ok_or_else(|| anyhow!("archive has no .sdPlugin folder"))?;
        let plugin = load_plugin(&dir, false, "en")?;
        let dest = dest_root.join(format!("{}.sdPlugin", plugin.uuid));
        if dest.exists() {
            std::fs::remove_dir_all(&dest)?;
        }
        std::fs::rename(&dir, &dest)?;
        Ok(plugin.uuid)
    })();
    std::fs::remove_dir_all(&staging).ok();
    res
}

pub fn uninstall(uuid: &str) -> Result<()> {
    let dir = paths::plugins_dir().join(format!("{uuid}.sdPlugin"));
    if !dir.exists() {
        bail!("{uuid} is not installed (built-in plugins cannot be removed)");
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

/// Restart budget: a plugin that keeps crashing is left stopped instead of
/// burning CPU. Five crashes within a minute stops it.
pub fn should_restart(started: Instant, restarts: u32) -> Option<Duration> {
    if started.elapsed() > Duration::from_secs(60) {
        return Some(Duration::from_secs(1));
    }
    if restarts >= 5 {
        return None;
    }
    Some(Duration::from_secs(1 << restarts.min(4)))
}
