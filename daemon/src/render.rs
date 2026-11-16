//! Turns a key or touch-strip description into pixels. Everything is composed
//! as one SVG document and rasterised with resvg, which gives titles real
//! system fonts (fontconfig names, bold/italic, underline, outlines) and lets
//! plugin images arrive as PNG, JPEG or SVG alike.

use base64::Engine as _;
use image::RgbaImage;
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum ImageSource {
    File(PathBuf),
    /// A `data:` URL or raw SVG markup, as plugins send them.
    Data(String),
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Title {
    pub text: String,
    pub align: String,
    pub color: String,
    pub family: String,
    pub size: f64,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub outline: f64,
}

impl Default for Title {
    fn default() -> Self {
        Title { text: String::new(), align: "bottom".into(), color: "#ffffff".into(), family: String::new(), size: 12.0, bold: false, italic: false, underline: false, outline: 2.0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum Overlay {
    None,
    Alert,
    Ok,
    /// Small dot used by Open Application while the app runs.
    Running,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct KeyVisual {
    pub image: Option<ImageSource>,
    pub background: String,
    pub title: Option<Title>,
    pub overlay: Overlay,
    pub pressed: bool,
    /// 0..1 progress ring drawn around the key edge (Timer, Delay).
    pub progress: Option<f64>,
}

impl Default for KeyVisual {
    fn default() -> Self {
        KeyVisual { image: None, background: "#000000".into(), title: None, overlay: Overlay::None, pressed: false, progress: None }
    }
}

fn fontdb() -> Arc<resvg::usvg::fontdb::Database> {
    static DB: OnceLock<Arc<resvg::usvg::fontdb::Database>> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = resvg::usvg::fontdb::Database::new();
        db.load_system_fonts();
        db.set_sans_serif_family(pick_family(&db, &["Inter", "Noto Sans", "DejaVu Sans", "Liberation Sans", "Cantarell"]));
        Arc::new(db)
    })
    .clone()
}

fn pick_family(db: &resvg::usvg::fontdb::Database, wanted: &[&str]) -> String {
    for w in wanted {
        if db.faces().any(|f| f.families.iter().any(|(n, _)| n.eq_ignore_ascii_case(w))) {
            return w.to_string();
        }
    }
    db.faces().next().and_then(|f| f.families.first().map(|(n, _)| n.clone())).unwrap_or_else(|| "sans-serif".into())
}

pub fn font_families() -> Vec<String> {
    let db = fontdb();
    let mut names: Vec<String> = db.faces().filter_map(|f| f.families.first().map(|(n, _)| n.clone())).collect();
    names.sort();
    names.dedup();
    names
}

struct CachedImage {
    modified: Option<SystemTime>,
    href: Arc<String>,
}

/// Decoded images are turned into data URIs once and reused, since the same
/// icon is drawn every time a page is shown.
fn image_href(src: &ImageSource) -> Option<Arc<String>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, CachedImage>>> = OnceLock::new();
    match src {
        ImageSource::Data(d) => {
            let d = d.trim();
            if d.starts_with("<svg") || d.starts_with("<?xml") {
                let b64 = base64::engine::general_purpose::STANDARD.encode(d.as_bytes());
                return Some(Arc::new(format!("data:image/svg+xml;base64,{b64}")));
            }
            if let Some(rest) = d.strip_prefix("data:image/svg+xml") {
                // Plugins often send `data:image/svg+xml;charset=utf8,<svg …>` unencoded.
                if let Some(idx) = rest.find(',') {
                    let meta = &rest[..idx];
                    let body = &rest[idx + 1..];
                    if !meta.contains("base64") {
                        let decoded = percent_encoding::percent_decode_str(body).decode_utf8_lossy().to_string();
                        let b64 = base64::engine::general_purpose::STANDARD.encode(decoded.as_bytes());
                        return Some(Arc::new(format!("data:image/svg+xml;base64,{b64}")));
                    }
                }
            }
            if d.starts_with("data:") {
                return Some(Arc::new(d.to_string()));
            }
            None
        }
        ImageSource::File(path) => {
            let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok();
            let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
            if let Some(hit) = cache.lock().unwrap().get(path) {
                if hit.modified == modified {
                    return Some(hit.href.clone());
                }
            }
            let bytes = std::fs::read(path).ok()?;
            let mime = sniff_mime(path, &bytes);
            let href = if mime == "image/gif" {
                // resvg cannot draw GIFs; the first frame is what the key shows.
                let img = image::load_from_memory(&bytes).ok()?;
                let mut png = Vec::new();
                img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).ok()?;
                format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png))
            } else {
                format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(&bytes))
            };
            let href = Arc::new(href);
            let mut c = cache.lock().unwrap();
            if c.len() > 512 {
                c.clear();
            }
            c.insert(path.clone(), CachedImage { modified, href: href.clone() });
            Some(href)
        }
    }
}

fn sniff_mime(path: &Path, bytes: &[u8]) -> &'static str {
    if bytes.starts_with(b"\x89PNG") {
        "image/png"
    } else if bytes.starts_with(&[0xff, 0xd8]) {
        "image/jpeg"
    } else if bytes.starts_with(b"GIF8") {
        "image/gif"
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        "image/webp"
    } else if path.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("svg")).unwrap_or(false) || bytes.starts_with(b"<") {
        "image/svg+xml"
    } else {
        "image/png"
    }
}

pub fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Accepts `#rgb`, `#rrggbb`, `#rrggbbaa` and CSS names; anything else is passed through.
fn color(c: &str, fallback: &str) -> String {
    let c = c.trim();
    if c.is_empty() {
        return fallback.into();
    }
    escape(c)
}

fn title_svg(t: &Title, w: f64, h: f64, scale: f64) -> String {
    if t.text.is_empty() {
        return String::new();
    }
    let size = (t.size.max(4.0)) * scale;
    let lines: Vec<&str> = t.text.split('\n').collect();
    let line_h = size * 1.12;
    let block = line_h * lines.len() as f64;
    let pad = 4.0 * scale;
    let first_baseline = match t.align.as_str() {
        "top" => pad + size * 0.9,
        "middle" => (h - block) / 2.0 + size * 0.9,
        _ => h - pad - block + size * 0.9,
    };
    let family = if t.family.is_empty() { "sans-serif".to_string() } else { format!("'{}', sans-serif", escape(&t.family)) };
    let mut out = String::new();
    let weight = if t.bold { "bold" } else { "normal" };
    let style = if t.italic { "italic" } else { "normal" };
    let deco = if t.underline { "underline" } else { "none" };
    let stroke = if t.outline > 0.0 { format!(r##" stroke="#000000" stroke-width="{:.2}" stroke-linejoin="round" paint-order="stroke""##, t.outline * scale) } else { String::new() };
    for (i, line) in lines.iter().enumerate() {
        let y = first_baseline + i as f64 * line_h;
        out.push_str(&format!(
            r#"<text x="{x:.2}" y="{y:.2}" font-family="{family}" font-size="{size:.2}" font-weight="{weight}" font-style="{style}" text-decoration="{deco}" text-anchor="middle" fill="{fill}"{stroke}>{text}</text>"#,
            x = w / 2.0,
            fill = color(&t.color, "#ffffff"),
            text = escape(line),
        ));
    }
    out
}

fn overlay_svg(o: Overlay, w: f64, h: f64) -> String {
    let s = w.min(h);
    match o {
        Overlay::None => String::new(),
        Overlay::Alert => format!(
            r##"<rect width="{w}" height="{h}" fill="#000" fill-opacity="0.55"/><g transform="translate({tx},{ty}) scale({k})"><path d="M50 8 L94 88 H6 Z" fill="#f5c400" stroke="#000" stroke-width="3" stroke-linejoin="round"/><rect x="45" y="34" width="10" height="32" rx="3" fill="#000"/><circle cx="50" cy="76" r="6" fill="#000"/></g>"##,
            tx = (w - s * 0.6) / 2.0,
            ty = (h - s * 0.6) / 2.0,
            k = s * 0.6 / 100.0
        ),
        Overlay::Ok => format!(
            r##"<rect width="{w}" height="{h}" fill="#000" fill-opacity="0.55"/><g transform="translate({tx},{ty}) scale({k})"><circle cx="50" cy="50" r="44" fill="#2fb455"/><path d="M28 52 L44 68 L74 36" fill="none" stroke="#fff" stroke-width="10" stroke-linecap="round" stroke-linejoin="round"/></g>"##,
            tx = (w - s * 0.6) / 2.0,
            ty = (h - s * 0.6) / 2.0,
            k = s * 0.6 / 100.0
        ),
        Overlay::Running => format!(r##"<circle cx="{cx}" cy="{cy}" r="{r}" fill="#3ecf6b" stroke="#000" stroke-width="{sw}"/>"##, cx = w - s * 0.1, cy = s * 0.1, r = s * 0.055, sw = s * 0.015),
    }
}

pub fn key_svg(v: &KeyVisual, px: u32) -> String {
    let w = px as f64;
    let scale = w / 72.0;
    let mut body = format!(r#"<rect width="{w}" height="{w}" fill="{}"/>"#, color(&v.background, "#000000"));
    if let Some(src) = &v.image {
        if let Some(href) = image_href(src) {
            body.push_str(&format!(r#"<image width="{w}" height="{w}" preserveAspectRatio="xMidYMid meet" href="{href}"/>"#));
        }
    }
    if let Some(t) = &v.title {
        body.push_str(&title_svg(t, w, w, scale));
    }
    if let Some(p) = v.progress {
        let p = p.clamp(0.0, 1.0);
        let inset = 2.0 * scale;
        let side = w - inset * 2.0;
        let len = side * 4.0;
        body.push_str(&format!(
            r##"<rect x="{inset}" y="{inset}" width="{side}" height="{side}" rx="{r}" fill="none" stroke="#ffffff" stroke-opacity="0.25" stroke-width="{sw}"/><rect x="{inset}" y="{inset}" width="{side}" height="{side}" rx="{r}" fill="none" stroke="#f5c400" stroke-width="{sw}" stroke-dasharray="{on} {len}" />"##,
            r = 6.0 * scale,
            sw = 3.0 * scale,
            on = len * p,
        ));
    }
    body.push_str(&overlay_svg(v.overlay, w, w));
    if v.pressed {
        // The original app shrinks the face while a key is held.
        body = format!(r##"<rect width="{w}" height="{w}" fill="#000"/><g transform="translate({o},{o}) scale(0.88)">{body}</g>"##, o = w * 0.06);
    }
    format!(r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{w}" height="{w}" viewBox="0 0 {w} {w}">{body}</svg>"#)
}

pub fn rasterize(svg: &str, width: u32, height: u32) -> RgbaImage {
    let opts = resvg::usvg::Options { fontdb: fontdb(), ..Default::default() };
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width.max(1), height.max(1)).unwrap();
    match resvg::usvg::Tree::from_str(svg, &opts) {
        Ok(tree) => {
            let size = tree.size();
            let sx = width as f32 / size.width();
            let sy = height as f32 / size.height();
            resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(sx, sy), &mut pixmap.as_mut());
        }
        Err(e) => tracing::warn!("render failed: {e}"),
    }
    // tiny-skia stores premultiplied alpha; devices and PNG want straight alpha.
    let mut img = RgbaImage::new(width.max(1), height.max(1));
    for (dst, src) in img.pixels_mut().zip(pixmap.pixels()) {
        let c = src.demultiply();
        *dst = image::Rgba([c.red(), c.green(), c.blue(), c.alpha()]);
    }
    img
}

pub fn render_key(v: &KeyVisual, px: u32) -> RgbaImage {
    rasterize(&key_svg(v, px), px, px)
}

pub fn png_bytes(img: &RgbaImage) -> Vec<u8> {
    let mut out = Vec::new();
    image::DynamicImage::ImageRgba8(img.clone()).write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).ok();
    out
}

// ---------------------------------------------------------------------------
// Touch strip layouts (Stream Deck +) and the Neo info bar.
//
// A layout is a list of items, each with a `key`, a `type` (`pixmap`, `text`,
// `bar`, `gbar`) and a `rect` of `[x, y, w, h]` inside the 200×100 segment.
// `setFeedback` merges values into items by key. The built-in layout ids
// (`$X1`, `$A0`, `$A1`, `$B1`, `$B2`, `$C1`) are the ones the SDK documents.
// ---------------------------------------------------------------------------

pub fn builtin_layout(id: &str) -> Option<Value> {
    let v = match id {
        "$X1" => serde_json::json!({"id":"$X1","items":[
            {"key":"title","type":"text","rect":[16,10,168,16],"font":{"size":16,"weight":600},"alignment":"center"},
            {"key":"icon","type":"pixmap","rect":[76,40,48,48]}]}),
        "$A0" => serde_json::json!({"id":"$A0","items":[
            {"key":"title","type":"text","rect":[16,10,168,16],"font":{"size":16,"weight":600},"alignment":"left"},
            {"key":"full-canvas","type":"pixmap","rect":[0,0,200,100]}]}),
        "$A1" => serde_json::json!({"id":"$A1","items":[
            {"key":"title","type":"text","rect":[16,10,168,16],"font":{"size":16,"weight":600},"alignment":"left"},
            {"key":"icon","type":"pixmap","rect":[16,40,48,48]},
            {"key":"value","type":"text","rect":[76,40,108,48],"font":{"size":24,"weight":600},"alignment":"right"}]}),
        "$B1" => serde_json::json!({"id":"$B1","items":[
            {"key":"title","type":"text","rect":[16,10,168,16],"font":{"size":16,"weight":600},"alignment":"left"},
            {"key":"icon","type":"pixmap","rect":[16,40,48,48]},
            {"key":"value","type":"text","rect":[76,40,108,24],"font":{"size":20,"weight":600},"alignment":"right"},
            {"key":"indicator","type":"bar","rect":[76,74,108,12],"value":0,"subtype":0,"border_w":0}]}),
        "$B2" => serde_json::json!({"id":"$B2","items":[
            {"key":"title","type":"text","rect":[16,10,168,16],"font":{"size":16,"weight":600},"alignment":"left"},
            {"key":"icon","type":"pixmap","rect":[16,40,48,48]},
            {"key":"value","type":"text","rect":[76,40,108,24],"font":{"size":20,"weight":600},"alignment":"right"},
            {"key":"indicator","type":"gbar","rect":[76,74,108,20],"value":0,"subtype":0,"bar_h":12}]}),
        "$C1" => serde_json::json!({"id":"$C1","items":[
            {"key":"title","type":"text","rect":[16,10,168,16],"font":{"size":16,"weight":600},"alignment":"left"},
            {"key":"icon1","type":"pixmap","rect":[16,40,24,24]},
            {"key":"icon2","type":"pixmap","rect":[16,66,24,24]},
            {"key":"indicator1","type":"bar","rect":[48,46,136,12],"value":0},
            {"key":"indicator2","type":"bar","rect":[48,72,136,12],"value":0}]}),
        _ => return None,
    };
    Some(v)
}

/// Loads a layout given its id or a plugin-relative JSON path.
pub fn load_layout(spec: &str, plugin_dir: Option<&Path>) -> Option<Value> {
    if let Some(l) = builtin_layout(spec) {
        return Some(l);
    }
    let dir = plugin_dir?;
    let path = dir.join(spec);
    // Layout files must stay inside the plugin that names them.
    let canon = path.canonicalize().ok()?;
    if !canon.starts_with(dir.canonicalize().ok()?) {
        return None;
    }
    serde_json::from_slice(&std::fs::read(canon).ok()?).ok()
}

/// Applies a `setFeedback` payload: string/number values set the item's
/// `value`; objects merge item properties.
pub fn merge_feedback(state: &mut Map<String, Value>, payload: &Value) {
    if let Some(obj) = payload.as_object() {
        for (k, v) in obj {
            let entry = state.entry(k.clone()).or_insert_with(|| Value::Object(Map::new()));
            match v {
                Value::Object(props) => {
                    if !entry.is_object() {
                        *entry = Value::Object(Map::new());
                    }
                    let e = entry.as_object_mut().unwrap();
                    for (pk, pv) in props {
                        e.insert(pk.clone(), pv.clone());
                    }
                }
                other => {
                    if !entry.is_object() {
                        *entry = Value::Object(Map::new());
                    }
                    entry.as_object_mut().unwrap().insert("value".into(), other.clone());
                }
            }
        }
    }
}

pub struct SegmentContext<'a> {
    pub plugin_dir: Option<&'a Path>,
    pub background: Option<ImageSource>,
}

pub fn segment_svg(layout: &Value, feedback: &Map<String, Value>, w: u32, h: u32, ctx: &SegmentContext) -> String {
    let (wf, hf) = (w as f64, h as f64);
    let sx = wf / 200.0;
    let sy = hf / 100.0;
    let mut body = format!(r##"<rect width="{wf}" height="{hf}" fill="#000"/>"##);
    if let Some(bg) = &ctx.background {
        if let Some(href) = image_href(bg) {
            body.push_str(&format!(r#"<image width="{wf}" height="{hf}" preserveAspectRatio="xMidYMid slice" href="{href}"/>"#));
        }
    }
    let mut items: Vec<Map<String, Value>> = layout.get("items").and_then(|i| i.as_array()).map(|a| a.iter().filter_map(|i| i.as_object().cloned()).collect()).unwrap_or_default();
    for item in items.iter_mut() {
        let key = item.get("key").and_then(|k| k.as_str()).unwrap_or("").to_string();
        if let Some(Value::Object(fb)) = feedback.get(&key) {
            for (k, v) in fb {
                item.insert(k.clone(), v.clone());
            }
        }
    }
    items.sort_by_key(|i| i.get("zOrder").and_then(|z| z.as_i64()).unwrap_or(0));
    for item in &items {
        if item.get("enabled").and_then(|e| e.as_bool()) == Some(false) {
            continue;
        }
        let rect: Vec<f64> = item.get("rect").and_then(|r| r.as_array()).map(|a| a.iter().filter_map(|v| v.as_f64()).collect()).unwrap_or_else(|| vec![0.0, 0.0, 200.0, 100.0]);
        if rect.len() < 4 {
            continue;
        }
        let (x, y, rw, rh) = (rect[0] * sx, rect[1] * sy, rect[2] * sx, rect[3] * sy);
        let opacity = item.get("opacity").and_then(|o| o.as_f64()).unwrap_or(1.0);
        let mut g = format!(r#"<g opacity="{opacity}">"#);
        if let Some(bg) = item.get("background").and_then(|b| b.as_str()) {
            g.push_str(&format!(r#"<rect x="{x}" y="{y}" width="{rw}" height="{rh}" fill="{}"/>"#, color(bg, "#000")));
        }
        match item.get("type").and_then(|t| t.as_str()).unwrap_or("") {
            "pixmap" => {
                let src = item.get("value").and_then(|v| v.as_str()).unwrap_or("");
                if !src.is_empty() {
                    let source = if src.starts_with("data:") || src.starts_with('<') {
                        Some(ImageSource::Data(src.to_string()))
                    } else {
                        ctx.plugin_dir.map(|d| ImageSource::File(resolve_plugin_image(d, src)))
                    };
                    if let Some(href) = source.as_ref().and_then(image_href) {
                        g.push_str(&format!(r#"<image x="{x}" y="{y}" width="{rw}" height="{rh}" preserveAspectRatio="xMidYMid meet" href="{href}"/>"#));
                    }
                }
            }
            "text" => {
                let text = match item.get("value") {
                    Some(Value::String(s)) => s.clone(),
                    Some(Value::Number(n)) => n.to_string(),
                    _ => String::new(),
                };
                if !text.is_empty() {
                    let font = item.get("font").cloned().unwrap_or(Value::Null);
                    let size = font.get("size").and_then(|s| s.as_f64()).unwrap_or(16.0) * sy;
                    let weight = font.get("weight").and_then(|s| s.as_f64()).unwrap_or(400.0);
                    let align = item.get("alignment").and_then(|a| a.as_str()).unwrap_or("center");
                    let (tx, anchor) = match align {
                        "left" => (x, "start"),
                        "right" => (x + rw, "end"),
                        _ => (x + rw / 2.0, "middle"),
                    };
                    let fill = item.get("color").and_then(|c| c.as_str()).unwrap_or("#ffffff");
                    g.push_str(&format!(
                        r#"<text x="{tx}" y="{ty}" font-family="sans-serif" font-size="{size}" font-weight="{weight}" text-anchor="{anchor}" fill="{fill}">{t}</text>"#,
                        ty = y + rh / 2.0 + size * 0.35,
                        fill = color(fill, "#ffffff"),
                        t = escape(&text)
                    ));
                }
            }
            kind @ ("bar" | "gbar") => {
                let range = item.get("range");
                let min = range.and_then(|r| r.get("min")).and_then(|v| v.as_f64()).unwrap_or(0.0);
                let max = range.and_then(|r| r.get("max")).and_then(|v| v.as_f64()).unwrap_or(100.0);
                let value = item.get("value").and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))).unwrap_or(0.0);
                let frac = if max > min { ((value - min) / (max - min)).clamp(0.0, 1.0) } else { 0.0 };
                let bar_h = if kind == "gbar" { item.get("bar_h").and_then(|v| v.as_f64()).unwrap_or(12.0) * sy } else { rh };
                let fill = item.get("bar_fill_c").and_then(|c| c.as_str()).unwrap_or("#ffffff");
                let bg = item.get("bar_bg_c").and_then(|c| c.as_str()).unwrap_or("#666666");
                let border = item.get("bar_border_c").and_then(|c| c.as_str()).unwrap_or("#ffffff");
                let bw = item.get("border_w").and_then(|v| v.as_f64()).unwrap_or(2.0) * sy;
                g.push_str(&format!(
                    r#"<rect x="{x}" y="{y}" width="{rw}" height="{bar_h}" rx="{r}" fill="{bg}" stroke="{border}" stroke-width="{bw}"/><rect x="{x}" y="{y}" width="{fw}" height="{bar_h}" rx="{r}" fill="{fill}"/>"#,
                    r = bar_h / 2.0,
                    fw = rw * frac,
                    bg = color(bg, "#666"),
                    border = color(border, "#fff"),
                    fill = color(fill, "#fff"),
                ));
                if kind == "gbar" {
                    let ix = x + rw * frac;
                    g.push_str(&format!(r##"<path d="M{ix} {t} l{a} {b} h{c} z" fill="#ffffff"/>"##, t = y + bar_h + 1.0, a = -4.0 * sx, b = 6.0 * sy, c = 8.0 * sx));
                }
            }
            _ => {}
        }
        g.push_str("</g>");
        body.push_str(&g);
    }
    format!(r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{wf}" height="{hf}" viewBox="0 0 {wf} {hf}">{body}</svg>"#)
}

/// SDK image paths omit the extension and may have `@2x` variants.
pub fn resolve_plugin_image(dir: &Path, rel: &str) -> PathBuf {
    let base = dir.join(rel);
    if base.extension().is_some() && base.exists() {
        return base;
    }
    for suffix in [".svg", "@2x.png", ".png", "@2x.jpg", ".jpg", ".gif", ""] {
        let p = PathBuf::from(format!("{}{}", base.display(), suffix));
        if p.is_file() {
            return p;
        }
    }
    base
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_title_and_image() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10" fill="#ff0000"/></svg>"##;
        let v = KeyVisual { image: Some(ImageSource::Data(svg.into())), title: Some(Title { text: "Hi".into(), ..Default::default() }), ..Default::default() };
        let img = render_key(&v, 72);
        assert_eq!(img.dimensions(), (72, 72));
        // The red image covers the key; the centre must be red.
        let p = img.get_pixel(36, 20);
        assert!(p[0] > 200 && p[1] < 50, "{p:?}");
    }

    #[test]
    fn feedback_merges() {
        let mut st = Map::new();
        merge_feedback(&mut st, &serde_json::json!({"value": "50%", "indicator": {"value": 30, "bar_fill_c": "#f00"}}));
        let layout = builtin_layout("$B1").unwrap();
        let svg = segment_svg(&layout, &st, 200, 100, &SegmentContext { plugin_dir: None, background: None });
        assert!(svg.contains("50%"));
        let img = rasterize(&svg, 200, 100);
        assert_eq!(img.dimensions(), (200, 100));
    }
}
