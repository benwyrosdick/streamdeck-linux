use std::path::PathBuf;

pub const APP_ID: &str = "streamdeck-linux";

/// `~/.local/share/streamdeck-linux`: profiles, plugins, icon packs, backups.
pub fn data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("STREAMDECK_DATA_DIR") {
        return PathBuf::from(dir);
    }
    dirs::data_dir().unwrap_or_else(|| PathBuf::from("/tmp")).join(APP_ID)
}

pub fn config_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("STREAMDECK_CONFIG_DIR") {
        return PathBuf::from(dir);
    }
    dirs::config_dir().unwrap_or_else(|| PathBuf::from("/tmp")).join(APP_ID)
}

pub fn runtime_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("STREAMDECK_RUNTIME_DIR") {
        return PathBuf::from(dir);
    }
    dirs::runtime_dir().unwrap_or_else(std::env::temp_dir).join(APP_ID)
}

pub fn profiles_dir() -> PathBuf {
    data_dir().join("ProfilesV2")
}

pub fn plugins_dir() -> PathBuf {
    data_dir().join("Plugins")
}

pub fn icon_packs_dir() -> PathBuf {
    data_dir().join("IconPacks")
}

pub fn backups_dir() -> PathBuf {
    data_dir().join("Backups")
}

pub fn plugin_data_dir(plugin_uuid: &str) -> PathBuf {
    data_dir().join("PluginData").join(plugin_uuid)
}

pub fn settings_file() -> PathBuf {
    config_dir().join("settings.json")
}

pub fn control_socket() -> PathBuf {
    runtime_dir().join("control.sock")
}

pub fn logs_dir() -> PathBuf {
    data_dir().join("logs")
}

/// Assets that ship with this port (built-in action icons, layouts, sounds).
/// Looked up next to the binary first so a development build works uninstalled.
pub fn resources_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("STREAMDECK_RESOURCES") {
        return PathBuf::from(dir);
    }
    if let Ok(exe) = std::env::current_exe() {
        for candidate in [
            exe.parent().map(|p| p.join("../../../resources")),
            exe.parent().map(|p| p.join("../share/streamdeck-linux")),
            exe.parent().map(|p| p.join("resources")),
        ]
        .into_iter()
        .flatten()
        {
            if candidate.join("builtin").is_dir() {
                return candidate.canonicalize().unwrap_or(candidate);
            }
        }
    }
    PathBuf::from("/usr/share/streamdeck-linux")
}
