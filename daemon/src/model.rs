//! On-disk data model. These types mirror the Stream Deck "ProfilesV2" layout so
//! profiles exported from the macOS or Windows app import unchanged, and every
//! struct keeps unknown keys in `extra` so a round-trip never drops data written
//! by a newer version of the original app.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

fn yes() -> bool {
    true
}

fn empty_object() -> Value {
    Value::Object(Map::new())
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct StateData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_title: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_alignment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_style: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_underline: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outline_thickness: Option<f64>,
    /// Relative to the page directory, e.g. `Images/ABC…Z.png`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct PluginRef {
    #[serde(rename = "Name", default)]
    pub name: String,
    #[serde(rename = "UUID", default)]
    pub uuid: String,
    #[serde(rename = "Version", default)]
    pub version: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One step list of a multi action. `Multi Action Switch` stores two of them.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Routine {
    #[serde(rename = "Actions", default)]
    pub actions: Vec<ActionData>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ActionData {
    #[serde(rename = "ActionID", default)]
    pub action_id: String,
    #[serde(rename = "UUID", default)]
    pub uuid: String,
    #[serde(rename = "Name", default)]
    pub name: String,
    #[serde(rename = "Settings", default = "empty_object")]
    pub settings: Value,
    #[serde(rename = "State", default)]
    pub state: u32,
    #[serde(rename = "States", default)]
    pub states: Vec<StateData>,
    #[serde(rename = "LinkedTitle", default = "yes")]
    pub linked_title: bool,
    #[serde(rename = "Plugin", skip_serializing_if = "Option::is_none")]
    pub plugin: Option<PluginRef>,
    /// Child actions of multi actions, key logic, action wheels and dial stacks.
    #[serde(rename = "Actions", skip_serializing_if = "Option::is_none")]
    pub actions: Option<Vec<Routine>>,
    #[serde(rename = "Resources", skip_serializing_if = "Option::is_none")]
    pub resources: Option<Value>,
    /// Encoder presentation: background, icon, trigger descriptions, layout.
    #[serde(rename = "Encoder", skip_serializing_if = "Option::is_none")]
    pub encoder: Option<Value>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl ActionData {
    pub fn settings_obj(&self) -> Map<String, Value> {
        self.settings.as_object().cloned().unwrap_or_default()
    }

    pub fn setting_str(&self, key: &str) -> String {
        match self.settings.get(key) {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Number(n)) => n.to_string(),
            Some(Value::Bool(b)) => b.to_string(),
            _ => String::new(),
        }
    }

    pub fn setting_i64(&self, key: &str, default: i64) -> i64 {
        match self.settings.get(key) {
            Some(Value::Number(n)) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)).unwrap_or(default),
            Some(Value::String(s)) => s.trim().parse().unwrap_or(default),
            Some(Value::Bool(b)) => *b as i64,
            _ => default,
        }
    }

    pub fn setting_f64(&self, key: &str, default: f64) -> f64 {
        match self.settings.get(key) {
            Some(Value::Number(n)) => n.as_f64().unwrap_or(default),
            Some(Value::String(s)) => s.trim().parse().unwrap_or(default),
            _ => default,
        }
    }

    pub fn setting_bool(&self, key: &str, default: bool) -> bool {
        match self.settings.get(key) {
            Some(Value::Bool(b)) => *b,
            Some(Value::Number(n)) => n.as_i64().map(|v| v != 0).unwrap_or(default),
            Some(Value::String(s)) => matches!(s.as_str(), "true" | "1" | "yes"),
            _ => default,
        }
    }

    pub fn current_state(&self) -> Option<&StateData> {
        self.states.get(self.state as usize).or_else(|| self.states.first())
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Controller {
    #[serde(rename = "Type", default)]
    pub kind: String,
    /// Keyed by `"column,row"`. Encoders use `"index,0"`.
    #[serde(rename = "Actions", default, deserialize_with = "null_as_default")]
    pub actions: BTreeMap<String, ActionData>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

fn null_as_default<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct PageData {
    #[serde(rename = "Name", default)]
    pub name: String,
    #[serde(rename = "Icon", default)]
    pub icon: String,
    #[serde(rename = "Controllers", default)]
    pub controllers: Vec<Controller>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl PageData {
    pub fn controller(&self, kind: &str) -> Option<&Controller> {
        self.controllers.iter().find(|c| c.kind == kind)
    }

    pub fn controller_mut(&mut self, kind: &str) -> &mut Controller {
        if let Some(i) = self.controllers.iter().position(|c| c.kind == kind) {
            return &mut self.controllers[i];
        }
        self.controllers.push(Controller { kind: kind.to_string(), ..Default::default() });
        self.controllers.last_mut().unwrap()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct DeviceRef {
    #[serde(rename = "Model", default)]
    pub model: String,
    #[serde(rename = "UUID", default)]
    pub uuid: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct PagesRef {
    #[serde(rename = "Current", default)]
    pub current: String,
    #[serde(rename = "Default", default, skip_serializing_if = "String::is_empty")]
    pub default: String,
    #[serde(rename = "Pages", default)]
    pub pages: Vec<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ProfileManifest {
    #[serde(rename = "Device", default)]
    pub device: DeviceRef,
    #[serde(rename = "Name", default)]
    pub name: String,
    #[serde(rename = "Pages", default)]
    pub pages: PagesRef,
    #[serde(rename = "Version", default = "v2")]
    pub version: String,
    /// Smart profile binding: the application whose focus activates this profile.
    #[serde(rename = "AppIdentifier", default, skip_serializing_if = "Option::is_none")]
    pub app_identifier: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

fn v2() -> String {
    "2.0".into()
}

/// Page and folder directories are named with the page UUID's 16 bytes in
/// base32hex, with `V` swapped for `W` and a trailing `Z`. This matches what
/// the original app writes, so imported profiles keep their directory names.
pub fn page_dir_name(id: &uuid::Uuid) -> String {
    const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHIJKLMNOPQRSTUW";
    let bytes = id.as_bytes();
    let mut out = String::with_capacity(27);
    let mut buffer: u32 = 0;
    let mut bits = 0;
    for b in bytes {
        buffer = (buffer << 8) | *b as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ALPHABET[((buffer >> bits) & 31) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(ALPHABET[((buffer << (5 - bits)) & 31) as usize] as char);
    }
    out.push('Z');
    out
}

pub fn page_id_from_dir(name: &str) -> Option<uuid::Uuid> {
    let body = name.strip_suffix('Z')?;
    let mut buffer: u64 = 0;
    let mut bits = 0;
    let mut out = Vec::with_capacity(16);
    for c in body.chars() {
        let v = match c {
            '0'..='9' => c as u64 - '0' as u64,
            'A'..='U' => c as u64 - 'A' as u64 + 10,
            'W' | 'V' => 31,
            _ => return None,
        };
        buffer = (buffer << 5) | v;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push(((buffer >> bits) & 0xff) as u8);
        }
    }
    if out.len() < 16 {
        return None;
    }
    uuid::Uuid::from_slice(&out[..16]).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_dir_matches_original_encoding() {
        let id: uuid::Uuid = "a9f2c431-c27a-4075-a3a9-559e46147bb5".parse().unwrap();
        assert_eq!(page_dir_name(&id), "L7PC8CE2F907B8T9AMF4C53RMKZ");
        let id: uuid::Uuid = "3bc15085-01d4-4e7f-8208-ff14934e4fef".parse().unwrap();
        assert_eq!(page_dir_name(&id), "7F0L1181QH77W0G8WSA96JIFTSZ");
        assert_eq!(page_id_from_dir("7F0L1181QH77W0G8WSA96JIFTSZ"), Some(id));
    }

    #[test]
    fn round_trip_keeps_unknown_keys() {
        let src = r#"{"ActionID":"x","UUID":"a.b","Name":"n","Settings":{"k":1},"State":0,"States":[{"Title":"t","Mystery":3}],"LinkedTitle":true,"Future":{"a":1}}"#;
        let a: ActionData = serde_json::from_str(src).unwrap();
        let back = serde_json::to_value(&a).unwrap();
        assert_eq!(back["Future"]["a"], 1);
        assert_eq!(back["States"][0]["Mystery"], 3);
    }
}
