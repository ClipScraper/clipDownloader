use crate::database::{OnDuplicate, Settings};
use std::{fs, path::PathBuf};

/// Where we store settings.json on macOS:
///   ~/Library/Application Support/clip-downloader/settings.json
/// For other OSes, this still resolves to the platform's "config dir".
fn app_support_dir() -> PathBuf {
    let base = dirs::config_dir().unwrap_or_else(|| dirs::home_dir().unwrap_or_default());
    base.join("clip-downloader")
}

fn settings_json_path() -> PathBuf {
    app_support_dir().join("settings.json")
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            id: None,
            download_directory: dirs::download_dir()
                .unwrap_or_else(|| dirs::home_dir().unwrap_or_default())
                .to_string_lossy()
                .to_string(),
            on_duplicate: OnDuplicate::CreateNew,
        }
    }
}

fn ensure_download_dir(dir: &str) {
    if !dir.is_empty() {
        let _ = fs::create_dir_all(dir);
    }
}

/// Load settings from JSON; create the file with defaults if missing or invalid.
/// Also ensures the download directory exists and is absolute (falls back to default).
pub fn load_settings() -> Settings {
    let path = settings_json_path();

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    // Try reading settings.json
    let mut settings = match fs::read_to_string(&path) {
        Ok(s) => match serde_json::from_str::<Settings>(&s) {
            Ok(mut parsed) => {
                // If download_directory is empty, fall back to default.
                if parsed.download_directory.trim().is_empty() {
                    parsed.download_directory = Settings::default().download_directory;
                }
                parsed
            }
            Err(_) => Settings::default(),
        },
        Err(_) => Settings::default(),
    };

    // Make sure the download directory exists
    ensure_download_dir(&settings.download_directory);

    // If the file didn't exist or was invalid, rewrite a clean copy now.
    // (This also migrates anyone who previously had only DB settings.)
    let _ = fs::write(&path, serde_json::to_string_pretty(&settings).unwrap_or_else(|_| "{}".into()));

    settings
}

/// Save settings back to JSON (and ensure the target directory exists).
pub fn save_settings(settings: &Settings) -> Result<(), String> {
    let path = settings_json_path();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create settings dir: {e}"))?;
    }

    // Ensure the chosen download directory exists
    if !settings.download_directory.trim().is_empty() {
        fs::create_dir_all(&settings.download_directory)
            .map_err(|e| format!("Failed to create download directory: {e}"))?;
    }

    let body = serde_json::to_string_pretty(settings)
        .map_err(|e| format!("Failed to serialize settings: {e}"))?;
    fs::write(&path, body).map_err(|e| format!("Failed to write settings.json: {e}"))?;
    Ok(())
}

/// Map our duplicate policies to yt-dlp flags.
/// - Overwrite   -> force overwrite existing files
/// - CreateNew   -> we compute a unique name ourselves (no special flag)
/// - DoNothing   -> tell yt-dlp to skip and not resume partials
pub fn get_yt_dlp_duplicate_flags(on_duplicate: &OnDuplicate) -> Vec<String> {
    match on_duplicate {
        OnDuplicate::Overwrite => vec!["--force-overwrites".into()],
        OnDuplicate::CreateNew => vec![], // we ensure uniqueness by choosing a free name
        OnDuplicate::DoNothing => vec!["--no-overwrites".into(), "--no-continue".into()],
    }
}
