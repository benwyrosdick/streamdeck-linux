use crate::paths;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct DeviceSettings {
    pub brightness: u8,
    /// Minutes of inactivity before the keys go dark; 0 never sleeps.
    pub sleep_minutes: u32,
    /// The profile the user chose; smart profiles return to it.
    pub profile: Option<String>,
    pub name: Option<String>,
}

impl Default for DeviceSettings {
    fn default() -> Self {
        DeviceSettings { brightness: 60, sleep_minutes: 0, profile: None, name: None }
    }
}

/// The Virtual Stream Deck can take any grid, like the original's.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct VirtualDeck {
    pub id: String,
    pub name: String,
    pub columns: u8,
    pub rows: u8,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub sdk_port: u16,
    pub language: String,
    pub devices: BTreeMap<String, DeviceSettings>,
    pub virtual_decks: Vec<VirtualDeck>,
    pub smart_profiles: bool,
    pub auto_backup: bool,
    pub backups_to_keep: usize,
    /// Show the key face shrinking while it is held.
    pub press_feedback: bool,
    /// Extra map from smart-profile identifiers to Hyprland window classes,
    /// for profiles imported from macOS whose AppIdentifier is a bundle path.
    pub app_aliases: BTreeMap<String, String>,
    pub extra: BTreeMap<String, Value>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            sdk_port: 28196,
            language: std::env::var("LANG").ok().and_then(|l| l.split(['_', '.']).next().map(String::from)).filter(|l| !l.is_empty() && l != "C").unwrap_or_else(|| "en".into()),
            devices: BTreeMap::new(),
            virtual_decks: vec![VirtualDeck { id: "virtual-1".into(), name: "Virtual Stream Deck".into(), columns: 8, rows: 4 }],
            smart_profiles: true,
            auto_backup: true,
            backups_to_keep: 10,
            press_feedback: true,
            app_aliases: BTreeMap::new(),
            extra: BTreeMap::new(),
        }
    }
}

impl Settings {
    pub fn load() -> Settings {
        std::fs::read(paths::settings_file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn save(&self) {
        let path = paths::settings_file();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).ok();
        }
        if let Ok(v) = serde_json::to_value(self) {
            if let Err(e) = crate::store::write_json_atomic(&path, &v) {
                tracing::warn!("saving settings: {e}");
            }
        }
    }

    pub fn device(&mut self, id: &str) -> &mut DeviceSettings {
        self.devices.entry(id.to_string()).or_default()
    }
}

pub fn load_global_settings(plugin: &str) -> Value {
    std::fs::read(paths::data_dir().join("GlobalSettings").join(format!("{plugin}.json")))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_else(|| Value::Object(Default::default()))
}

pub fn save_global_settings(plugin: &str, value: &Value) {
    let dir = paths::data_dir().join("GlobalSettings");
    std::fs::create_dir_all(&dir).ok();
    crate::store::write_json_atomic(&dir.join(format!("{plugin}.json")), value).ok();
}
