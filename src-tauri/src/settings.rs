use crate::database::{Database, OnDuplicate, Settings};
use rusqlite::Result;

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

pub fn load_settings() -> Settings {
    match Database::new() {
        Ok(db) => db.get_settings().unwrap_or_else(|_| Settings::default()),
        Err(_) => Settings::default(),
    }
}

pub fn save_settings(settings: &Settings) -> Result<(), String> {
    Database::new()
        .map_err(|e| format!("Failed to connect to database: {}", e))?
        .update_settings(settings)
        .map_err(|e| format!("Failed to save settings: {}", e))?;

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
