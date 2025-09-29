use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum OnDuplicate {
    Overwrite,
    CreateNew,
    DoNothing,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Settings {
    pub download_directory: String,
    pub on_duplicate: OnDuplicate,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            download_directory: dirs::download_dir()
                .unwrap_or_else(|| dirs::home_dir().unwrap_or_default())
                .to_string_lossy()
                .to_string(),
            on_duplicate: OnDuplicate::CreateNew,
        }
    }
}

fn get_settings_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("clip-downloader").join("settings.json"))
}

pub fn load_settings() -> Settings {
    get_settings_path()
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_else(|| {
            let default_settings = Settings::default();
            if let Some(path) = get_settings_path() {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).ok();
                }
                save_settings(&default_settings).ok();
            }
            default_settings
        })
}

pub fn save_settings(settings: &Settings) -> Result<(), String> {
    get_settings_path()
        .ok_or_else(|| "Could not determine settings path".to_string())
        .and_then(|path| {
            if let Some(parent) = path.parent() {
                if !parent.exists() {
                    fs::create_dir_all(parent)
                        .map_err(|e| format!("Failed to create settings directory: {}", e))?;
                }
            }
            let content = serde_json::to_string_pretty(settings)
                .map_err(|e| format!("Failed to serialize settings: {}", e))?;
            fs::write(&path, content)
                .map_err(|e| format!("Failed to write settings file: {}", e))
        })
}

/// Get yt-dlp flags for handling duplicates
pub fn get_yt_dlp_duplicate_flags(on_duplicate: &OnDuplicate) -> Vec<String> {
    match on_duplicate {
        OnDuplicate::Overwrite => vec![],
        OnDuplicate::CreateNew => vec!["--no-overwrites".into()],
        OnDuplicate::DoNothing => vec!["--no-overwrites".into(), "--no-continue".into()],
    }
}
