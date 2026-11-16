//! Static description of every Stream Deck model: grid size, encoders, screens
//! and the identifiers the SDK and profile files use for it.

use serde::Serialize;

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct DeviceSpec {
    /// Stable short id used by this port (`mk2`, `plus`, …).
    pub id: &'static str,
    pub name: &'static str,
    pub columns: u8,
    pub rows: u8,
    pub encoders: u8,
    /// Neo's two capacitive page buttons.
    pub touch_points: u8,
    /// Touch strip (Plus) or info bar (Neo) in pixels.
    pub lcd: Option<(u32, u32)>,
    pub key_px: u32,
    pub visual: bool,
    /// SDK `DeviceType` number reported to plugins.
    pub sdk_type: u8,
    /// `Device.Model` written into profile manifests. Codes for the 15-key,
    /// Mini, XL, Plus and Neo are taken from the original app's default
    /// profiles; Pedal and Plus XL are the app's model strings for those
    /// families but are not confirmed against a shipped profile.
    pub model_code: &'static str,
}

impl DeviceSpec {
    pub fn key_count(&self) -> u8 {
        self.columns * self.rows
    }
}

pub const SPECS: &[DeviceSpec] = &[
    DeviceSpec { id: "original", name: "Stream Deck", columns: 5, rows: 3, encoders: 0, touch_points: 0, lcd: None, key_px: 72, visual: true, sdk_type: 0, model_code: "20GAA9902" },
    DeviceSpec { id: "mk2", name: "Stream Deck MK.2", columns: 5, rows: 3, encoders: 0, touch_points: 0, lcd: None, key_px: 72, visual: true, sdk_type: 0, model_code: "20GAA9902" },
    DeviceSpec { id: "mini", name: "Stream Deck Mini", columns: 3, rows: 2, encoders: 0, touch_points: 0, lcd: None, key_px: 80, visual: true, sdk_type: 1, model_code: "20GAI9901" },
    DeviceSpec { id: "xl", name: "Stream Deck XL", columns: 8, rows: 4, encoders: 0, touch_points: 0, lcd: None, key_px: 96, visual: true, sdk_type: 2, model_code: "20GAT9902" },
    DeviceSpec { id: "pedal", name: "Stream Deck Pedal", columns: 3, rows: 1, encoders: 0, touch_points: 0, lcd: None, key_px: 0, visual: false, sdk_type: 5, model_code: "20GBF9901" },
    DeviceSpec { id: "plus", name: "Stream Deck +", columns: 4, rows: 2, encoders: 4, touch_points: 0, lcd: Some((800, 100)), key_px: 120, visual: true, sdk_type: 7, model_code: "20GBD9901" },
    DeviceSpec { id: "plusxl", name: "Stream Deck + XL", columns: 9, rows: 4, encoders: 6, touch_points: 0, lcd: Some((1200, 100)), key_px: 112, visual: true, sdk_type: 7, model_code: "20GBX9901" },
    DeviceSpec { id: "neo", name: "Stream Deck Neo", columns: 4, rows: 2, encoders: 0, touch_points: 2, lcd: Some((248, 58)), key_px: 96, visual: true, sdk_type: 9, model_code: "20GBJ9901" },
];

pub fn spec(id: &str) -> Option<&'static DeviceSpec> {
    SPECS.iter().find(|s| s.id == id)
}

pub fn spec_for_kind(kind: elgato_streamdeck::info::Kind) -> &'static DeviceSpec {
    use elgato_streamdeck::info::Kind::*;
    let id = match kind {
        Original | OriginalV2 => "original",
        Mk2 | Mk2Scissor | Mk2Module | Mk2ModuleScissor => "mk2",
        Mini | MiniMk2 | MiniDiscord | MiniMk2Module => "mini",
        Xl | XlV2 | XlV2Module => "xl",
        Pedal => "pedal",
        Plus => "plus",
        PlusXl => "plusxl",
        Neo => "neo",
    };
    spec(id).unwrap()
}

/// Finds the spec a profile was made for, falling back on a model with the
/// same family prefix (e.g. `20GAA9901` is the earlier 15-key revision).
pub fn spec_for_model(model: &str) -> Option<&'static DeviceSpec> {
    if let Some(s) = SPECS.iter().find(|s| s.model_code == model) {
        return Some(s);
    }
    let family = model.get(..5)?;
    SPECS.iter().find(|s| s.model_code.starts_with(family))
}

/// Whether profiles made for `a` can drive `b` without moving keys.
pub fn layout_compatible(a: &DeviceSpec, b: &DeviceSpec) -> bool {
    a.columns == b.columns && a.rows == b.rows && a.encoders == b.encoders
}
