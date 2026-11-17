//! Desktop integration: applications, windows (via Hyprland), URLs, files.

use anyhow::{anyhow, bail, Result};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

/// Starts a program detached from the daemon so it survives a daemon restart
/// and never becomes a zombie child.
pub fn spawn_detached(program: &str, args: &[String]) -> Result<()> {
    let mut cmd = std::process::Command::new("setsid");
    cmd.arg("-f").arg(program).args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    cmd.status().map_err(|e| anyhow!("could not start {program}: {e}"))?;
    Ok(())
}

pub fn spawn_shell(command: &str) -> Result<()> {
    spawn_detached("sh", &["-c".into(), command.into()])
}

/// The Open action stores a path that may be quoted and may carry arguments,
/// as in `"/usr/bin/foo" --bar`.
pub fn open_path(raw: &str) -> Result<()> {
    let raw = raw.trim();
    if raw.is_empty() {
        bail!("no path set");
    }
    let (path, rest) = if let Some(stripped) = raw.strip_prefix('"') {
        match stripped.find('"') {
            Some(end) => (stripped[..end].to_string(), stripped[end + 1..].trim().to_string()),
            None => (stripped.to_string(), String::new()),
        }
    } else {
        (raw.to_string(), String::new())
    };
    let expanded = expand_home(&path);
    let p = Path::new(&expanded);
    if path.contains("://") {
        return spawn_detached("xdg-open", &[path]);
    }
    if p.extension().and_then(|e| e.to_str()) == Some("desktop") && p.exists() {
        return launch_desktop_file(p, &[]);
    }
    if p.exists() && is_executable(p) && !p.is_dir() {
        let args = shell_words(&rest);
        return spawn_detached(&expanded, &args);
    }
    if p.exists() {
        return spawn_detached("xdg-open", &[expanded]);
    }
    if path.ends_with(".app") || path.starts_with("/Applications/") || path.starts_with("/System/") {
        bail!("{path} is a macOS application path; pick a Linux application or file instead");
    }
    // Not a file: treat it as a command line, which is what users of the
    // Windows app often type here.
    spawn_shell(raw)
}

fn expand_home(p: &str) -> String {
    if let Some(rest) = p.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest).to_string_lossy().to_string();
        }
    }
    p.to_string()
}

fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).map(|m| m.permissions().mode() & 0o111 != 0).unwrap_or(false)
}

pub fn shell_words(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut any = false;
    for c in s.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => cur.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                any = true
            }
            (None, c) if c.is_whitespace() => {
                if !cur.is_empty() || any {
                    out.push(std::mem::take(&mut cur));
                    any = false;
                }
            }
            (None, c) => cur.push(c),
        }
    }
    if !cur.is_empty() || any {
        out.push(cur);
    }
    out
}

pub fn open_url(url: &str, browser: Option<&str>) -> Result<()> {
    let mut url = url.trim().to_string();
    if url.is_empty() {
        bail!("no URL set");
    }
    if !url.contains("://") {
        url = format!("https://{url}");
    }
    match browser.filter(|b| !b.trim().is_empty()) {
        Some(b) if b.ends_with(".desktop") => launch_desktop_file(Path::new(b), &[url]),
        Some(b) => spawn_detached(b, &[url]),
        None => spawn_detached("xdg-open", &[url]),
    }
}

/// "Access in background": request the URL without opening a browser, for
/// webhooks and home-automation endpoints.
pub async fn fetch_url(url: &str) -> Result<()> {
    let mut url = url.trim().to_string();
    if !url.contains("://") {
        url = format!("https://{url}");
    }
    let status = Command::new("curl").args(["-fsSL", "--max-time", "15", "-o", "/dev/null", &url]).status().await.map_err(|e| anyhow!("curl: {e}"))?;
    if !status.success() {
        bail!("request to {url} failed ({status})");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Desktop entries
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopApp {
    pub id: String,
    pub name: String,
    pub exec: String,
    pub icon: String,
    pub icon_path: Option<String>,
    pub path: String,
    pub wm_class: String,
}

fn data_dirs() -> Vec<PathBuf> {
    let mut dirs_out = Vec::new();
    if let Some(d) = dirs::data_dir() {
        dirs_out.push(d);
    }
    let sys = std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    for d in sys.split(':').filter(|d| !d.is_empty()) {
        dirs_out.push(PathBuf::from(d));
    }
    dirs_out.push(PathBuf::from("/var/lib/flatpak/exports/share"));
    if let Some(h) = dirs::home_dir() {
        dirs_out.push(h.join(".local/share/flatpak/exports/share"));
    }
    dirs_out
}

fn parse_desktop(path: &Path) -> Option<DesktopApp> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut in_entry = false;
    let mut kv: HashMap<String, String> = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            kv.entry(k.trim().to_string()).or_insert_with(|| v.trim().to_string());
        }
    }
    if kv.get("Type").map(|t| t != "Application").unwrap_or(true) || kv.get("NoDisplay").map(|v| v == "true").unwrap_or(false) || kv.get("Hidden").map(|v| v == "true").unwrap_or(false) {
        return None;
    }
    let id = path.file_name()?.to_string_lossy().trim_end_matches(".desktop").to_string();
    let icon = kv.get("Icon").cloned().unwrap_or_default();
    Some(DesktopApp {
        name: kv.get("Name").cloned().unwrap_or_else(|| id.clone()),
        exec: kv.get("Exec").cloned().unwrap_or_default(),
        icon_path: find_icon(&icon).map(|p| p.to_string_lossy().to_string()),
        icon,
        path: path.to_string_lossy().to_string(),
        wm_class: kv.get("StartupWMClass").cloned().unwrap_or_default(),
        id,
    })
}

pub fn list_applications() -> Vec<DesktopApp> {
    let mut seen = HashMap::new();
    for d in data_dirs() {
        let Ok(rd) = std::fs::read_dir(d.join("applications")) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("desktop") {
                continue;
            }
            let id = e.file_name().to_string_lossy().to_string();
            if seen.contains_key(&id) {
                continue; // earlier data dirs shadow later ones
            }
            if let Some(app) = parse_desktop(&p) {
                seen.insert(id, app);
            }
        }
    }
    let mut apps: Vec<DesktopApp> = seen.into_values().collect();
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}

pub fn find_app(key: &str) -> Option<DesktopApp> {
    if key.is_empty() {
        return None;
    }
    let p = Path::new(key);
    if p.extension().and_then(|e| e.to_str()) == Some("desktop") && p.exists() {
        return parse_desktop(p);
    }
    let id = key.trim_end_matches(".desktop");
    list_applications().into_iter().find(|a| a.id == id || a.name.eq_ignore_ascii_case(key) || a.path == key)
}

/// Looks an icon name up in the common themes, preferring scalable art.
pub fn find_icon(name: &str) -> Option<PathBuf> {
    if name.is_empty() {
        return None;
    }
    let p = Path::new(name);
    if p.is_absolute() {
        return p.exists().then(|| p.to_path_buf());
    }
    let mut themes = vec!["hicolor".to_string(), "Adwaita".to_string(), "Papirus".to_string(), "Papirus-Dark".to_string(), "Yaru".to_string(), "breeze".to_string()];
    if let Ok(t) = std::fs::read_to_string(dirs::config_dir().unwrap_or_default().join("gtk-3.0/settings.ini")) {
        if let Some(line) = t.lines().find(|l| l.starts_with("gtk-icon-theme-name")) {
            if let Some((_, v)) = line.split_once('=') {
                themes.insert(0, v.trim().to_string());
            }
        }
    }
    let sizes = ["scalable", "512x512", "256x256", "192x192", "128x128", "96x96", "64x64", "48x48"];
    let mut bases: Vec<PathBuf> = data_dirs().into_iter().map(|d| d.join("icons")).collect();
    if let Some(h) = dirs::home_dir() {
        bases.insert(0, h.join(".icons"));
    }
    for base in &bases {
        for theme in &themes {
            for size in sizes {
                for ext in ["svg", "png"] {
                    let c = base.join(theme).join(size).join("apps").join(format!("{name}.{ext}"));
                    if c.exists() {
                        return Some(c);
                    }
                }
            }
        }
    }
    for ext in ["svg", "png", "xpm"] {
        let c = PathBuf::from(format!("/usr/share/pixmaps/{name}.{ext}"));
        if c.exists() {
            return Some(c);
        }
    }
    None
}

/// Runs a desktop entry's Exec line with field codes removed.
pub fn launch_desktop_file(path: &Path, extra: &[String]) -> Result<()> {
    let app = parse_desktop(path).ok_or_else(|| anyhow!("{} is not a launchable desktop entry", path.display()))?;
    launch_app(&app, extra)
}

pub fn launch_app(app: &DesktopApp, extra: &[String]) -> Result<()> {
    // gtk-launch honours DBusActivatable, Terminal=true and the rest of the spec.
    if extra.is_empty() && which("gtk-launch") {
        return spawn_detached("gtk-launch", &[app.id.clone()]);
    }
    let mut words: Vec<String> = Vec::new();
    for w in shell_words(&app.exec) {
        match w.as_str() {
            "%f" | "%F" | "%u" | "%U" => words.extend(extra.iter().cloned()),
            "%i" | "%c" | "%k" | "%d" | "%D" | "%n" | "%N" | "%v" | "%m" => {}
            _ => words.push(w.replace("%%", "%")),
        }
    }
    if words.is_empty() {
        bail!("{} has no Exec line", app.name);
    }
    let program = words.remove(0);
    spawn_detached(&program, &words)
}

pub fn which(bin: &str) -> bool {
    std::env::var_os("PATH").map(|paths| std::env::split_paths(&paths).any(|p| p.join(bin).is_file())).unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Hyprland
// ---------------------------------------------------------------------------

fn hypr_socket(name: &str) -> Option<PathBuf> {
    let sig = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok()?;
    let runtime = dirs::runtime_dir()?;
    let p = runtime.join("hypr").join(sig).join(name);
    p.exists().then_some(p)
}

pub async fn hyprctl_json(args: &[&str]) -> Option<Value> {
    let out = Command::new("hyprctl").arg("-j").args(args).output().await.ok()?;
    serde_json::from_slice(&out.stdout).ok()
}

pub async fn active_window_class() -> Option<String> {
    let v = hyprctl_json(&["activewindow"]).await?;
    v.get("class").and_then(|c| c.as_str()).map(String::from)
}

#[derive(Clone, Debug)]
pub struct Window {
    pub address: String,
    pub class: String,
    pub pid: i64,
}

pub async fn windows() -> Vec<Window> {
    hyprctl_json(&["clients"])
        .await
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .map(|c| Window {
            address: c.get("address").and_then(|a| a.as_str()).unwrap_or("").to_string(),
            class: c.get("class").and_then(|a| a.as_str()).unwrap_or("").to_string(),
            pid: c.get("pid").and_then(|a| a.as_i64()).unwrap_or(0),
        })
        .collect()
}

/// Identifiers a window of this application may carry: its StartupWMClass,
/// desktop id, and the program name from Exec.
pub fn app_classes(app: &DesktopApp) -> Vec<String> {
    let mut v = Vec::new();
    if !app.wm_class.is_empty() {
        v.push(app.wm_class.to_lowercase());
    }
    v.push(app.id.to_lowercase());
    if let Some(last) = app.id.rsplit('.').next() {
        v.push(last.to_lowercase());
    }
    if let Some(first) = shell_words(&app.exec).into_iter().find(|w| !w.contains('=') && w != "env") {
        if let Some(name) = Path::new(&first).file_name() {
            v.push(name.to_string_lossy().to_lowercase());
        }
    }
    v.dedup();
    v
}

pub async fn app_windows(app: &DesktopApp) -> Vec<Window> {
    let classes = app_classes(app);
    windows().await.into_iter().filter(|w| classes.contains(&w.class.to_lowercase())).collect()
}

pub async fn hyprctl_dispatch(args: &[&str]) -> Result<()> {
    let out = Command::new("hyprctl").arg("dispatch").args(args).output().await?;
    let text = String::from_utf8_lossy(&out.stdout);
    if !out.status.success() || text.trim().starts_with("error") {
        bail!("hyprctl dispatch {}: {}", args.join(" "), text.trim());
    }
    Ok(())
}

pub async fn focus_window(w: &Window) -> Result<()> {
    hyprctl_dispatch(&["focuswindow", &format!("address:{}", w.address)]).await
}

pub async fn close_window(w: &Window) -> Result<()> {
    hyprctl_dispatch(&["closewindow", &format!("address:{}", w.address)]).await
}

pub fn kill_pid(pid: i64, force: bool) {
    if pid > 0 {
        unsafe {
            libc::kill(pid as i32, if force { libc::SIGKILL } else { libc::SIGTERM });
        }
    }
}

/// Follows the focused window's class. Used for smart profiles and for the
/// SDK's applicationDidLaunch / applicationDidTerminate events.
pub fn watch_active_window(tx: tokio::sync::mpsc::UnboundedSender<HyprEvent>) {
    tokio::spawn(async move {
        loop {
            if let Some(path) = hypr_socket(".socket2.sock") {
                if let Ok(stream) = tokio::net::UnixStream::connect(&path).await {
                    if let Some(c) = active_window_class().await {
                        tx.send(HyprEvent::ActiveClass(c)).ok();
                    }
                    let mut lines = BufReader::new(stream).lines();
                    while let Ok(Some(line)) = lines.next_line().await {
                        if let Some(rest) = line.strip_prefix("activewindow>>") {
                            let class = rest.split(',').next().unwrap_or("").to_string();
                            tx.send(HyprEvent::ActiveClass(class)).ok();
                        } else if line.starts_with("openwindow>>") || line.starts_with("closewindow>>") {
                            tx.send(HyprEvent::WindowsChanged).ok();
                        }
                    }
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    });
}

#[derive(Debug, Clone)]
pub enum HyprEvent {
    ActiveClass(String),
    WindowsChanged,
}

/// Process names currently running, for plugins that monitor applications.
pub fn running_process_names() -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    if let Ok(rd) = std::fs::read_dir("/proc") {
        for e in rd.flatten() {
            let name = e.file_name();
            if !name.to_string_lossy().bytes().all(|b| b.is_ascii_digit()) {
                continue;
            }
            if let Ok(comm) = std::fs::read_to_string(e.path().join("comm")) {
                out.insert(comm.trim().to_string());
            }
        }
    }
    out
}

/// systemd-logind tells us when the machine wakes, which the SDK reports as
/// `systemDidWakeUp`. `gdbus monitor` avoids a D-Bus client dependency.
pub fn watch_sleep(tx: tokio::sync::mpsc::UnboundedSender<bool>) {
    tokio::spawn(async move {
        let Ok(mut child) = Command::new("gdbus")
            .args(["monitor", "--system", "--dest", "org.freedesktop.login1", "--object-path", "/org/freedesktop/login1"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
        else {
            return;
        };
        let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if line.contains("PrepareForSleep") {
                tx.send(line.contains("true")).ok();
            }
        }
    });
}

/// Runs `wpctl` (WirePlumber) against the default microphone.
async fn wpctl(args: &[&str]) -> Result<String> {
    let out = Command::new("wpctl").args(args).stdin(Stdio::null()).output().await.map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => anyhow!("wpctl not found; install wireplumber"),
        _ => anyhow!("wpctl: {e}"),
    })?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let err = err.trim();
        bail!("{}", if err.is_empty() { "no microphone" } else { err });
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub const MIC: &str = "@DEFAULT_AUDIO_SOURCE@";

/// Whether the default microphone is muted; None when there is none.
pub async fn mic_muted() -> Option<bool> {
    parse_muted(&wpctl(&["get-volume", MIC]).await.ok()?)
}

fn parse_muted(out: &str) -> Option<bool> {
    out.trim().starts_with("Volume:").then(|| out.contains("[MUTED]"))
}

/// Mutes, unmutes, or (None) toggles the default microphone.
pub async fn set_mic_muted(mute: Option<bool>) -> Result<()> {
    let arg = match mute {
        Some(true) => "1",
        Some(false) => "0",
        None => "toggle",
    };
    wpctl(&["set-mute", MIC, arg]).await.map(|_| ())
}

/// Turns the default microphone's level up or down, never past 100 %.
pub async fn step_mic_volume(percent: i32) -> Result<()> {
    let step = format!("{}%{}", percent.unsigned_abs(), if percent < 0 { "-" } else { "+" });
    wpctl(&["set-volume", "-l", "1.0", MIC, &step]).await.map(|_| ())
}

/// Signals whenever a microphone or the default devices change, so mute keys
/// follow mutes made elsewhere. `pactl subscribe` works with PipeWire's
/// PulseAudio server and avoids a client library. It reconnects when the
/// sound server restarts.
pub fn watch_audio(tx: tokio::sync::mpsc::UnboundedSender<()>) {
    tokio::spawn(async move {
        loop {
            let Ok(mut child) = Command::new("pactl").arg("subscribe").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true).spawn() else {
                return;
            };
            let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if (line.contains(" on source #") || line.contains(" on server")) && tx.send(()).is_err() {
                    return;
                }
            }
            child.wait().await.ok();
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            if tx.send(()).is_err() {
                return;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words() {
        assert_eq!(shell_words(r#"a "b c" 'd' e"#), vec!["a", "b c", "d", "e"]);
    }

    #[test]
    fn mic_mute_state() {
        assert_eq!(parse_muted("Volume: 1.00\n"), Some(false));
        assert_eq!(parse_muted("Volume: 0.45 [MUTED]\n"), Some(true));
        assert_eq!(parse_muted(""), None);
    }
}
