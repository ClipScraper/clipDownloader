use crate::database::{DefaultOutput, DeleteMode, OnDuplicate, Settings};
use std::{
    fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;

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
            download_directory: default_download_dir().to_string_lossy().to_string(),
            on_duplicate: OnDuplicate::CreateNew,
            delete_mode: DeleteMode::Soft,
            debug_logs: false,
            default_output: DefaultOutput::Video,
            download_automatically: true,
            keep_downloading_on_other_pages: true,
            parallel_downloads: 3,
            use_system_binaries: false,
            cooldown_secs: 0,
            retry_on_queue_empty: false,
        }
    }
}

fn default_download_dir() -> PathBuf {
    // Cross-platform Downloads folder (dirs::download_dir handles win/mac/linux)
    dirs::download_dir().or_else(|| dirs::home_dir()).unwrap_or_else(|| PathBuf::from("/"))
}

fn dir_is_writable(p: &Path) -> bool {
    if !p.exists() || !p.is_dir() {
        return false;
    }
    let test = p.join(format!(".writecheck-{}.tmp", Uuid::new_v4()));
    match fs::OpenOptions::new().create(true).write(true).truncate(true).open(&test) {
        Ok(_) => {
            let _ = fs::remove_file(test);
            true
        }
        Err(_) => false,
    }
}

/// Validate a candidate directory. If invalid, return the default Downloads dir (and try to create it).
fn validated_download_dir<S: Into<String>>(candidate: S) -> String {
    let cand = candidate.into();
    let mut path = PathBuf::from(cand.trim());

    // Empty or non-absolute? → default
    if path.as_os_str().is_empty() || !path.is_absolute() {
        path = default_download_dir();
    }

    // Try to create if missing; if that fails, revert to default
    if !path.exists() {
        if let Err(_) = fs::create_dir_all(&path) {
            path = default_download_dir();
            let _ = fs::create_dir_all(&path);
        }
    }

    // If still not writable, revert to default
    if !dir_is_writable(&path) {
        let d = default_download_dir();
        let _ = fs::create_dir_all(&d);
        return d.to_string_lossy().to_string();
    }

    path.to_string_lossy().to_string()
}

/// Read settings without rewriting them or probing the download directory.
/// Readers must not race a save or replace an unreadable file with defaults.
pub fn load_settings() -> Settings {
    read_settings(&settings_json_path())
}

fn read_settings(path: &Path) -> Settings {
    let mut settings: Settings = match fs::read_to_string(path) {
        Ok(body) => serde_json::from_str(&body).unwrap_or_else(|error| {
            tracing::warn!("Could not parse settings: {error}");
            Settings::default()
        }),
        Err(error) => {
            if error.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!("Could not read settings: {error}");
            }
            Settings::default()
        }
    };
    if !Path::new(&settings.download_directory).is_absolute() {
        settings.download_directory = default_download_dir().to_string_lossy().to_string();
    }
    settings
}

fn write_settings(path: &Path, settings: &Settings) -> Result<(), String> {
    use std::io::Write;

    let parent = path.parent().ok_or_else(|| "Settings path has no parent".to_string())?;
    fs::create_dir_all(parent).map_err(|e| format!("Failed to create settings dir: {e}"))?;
    let body = serde_json::to_vec_pretty(settings).map_err(|e| format!("Failed to serialize settings: {e}"))?;
    // Same-directory temporary file makes replacement atomic for concurrent readers.
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    file.write_all(&body).map_err(|e| e.to_string())?;
    file.as_file().sync_all().map_err(|e| e.to_string())?;
    file.persist(path).map_err(|e| format!("Failed to replace settings.json: {e}"))?;
    Ok(())
}

/// Save settings back to JSON (and ensure the target directory exists).
/// Also validates the directory; if invalid, we fall back to the default Downloads path.
pub fn save_settings(settings: &Settings) -> Result<(), String> {
    let path = settings_json_path();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create settings dir: {e}"))?;
    }

    let final_dir = validated_download_dir(&settings.download_directory);
    let to_write = Settings {
        id: settings.id,
        download_directory: final_dir,
        on_duplicate: settings.on_duplicate.clone(),
        delete_mode: settings.delete_mode.clone(),
        debug_logs: settings.debug_logs,
        default_output: settings.default_output.clone(),
        download_automatically: settings.download_automatically,
        keep_downloading_on_other_pages: settings.keep_downloading_on_other_pages,
        parallel_downloads: settings.parallel_downloads,
        use_system_binaries: settings.use_system_binaries,
        cooldown_secs: settings.cooldown_secs,
        retry_on_queue_empty: settings.retry_on_queue_empty,
    };

    write_settings(&path, &to_write)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reading_settings_never_overwrites_invalid_or_missing_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        read_settings(&path);
        assert!(!path.exists());
        fs::write(&path, "invalid original data").unwrap();
        read_settings(&path);
        assert_eq!(fs::read_to_string(&path).unwrap(), "invalid original data");
    }

    #[test]
    fn settings_replacement_round_trips_and_leaves_no_temp_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let mut settings = Settings::default();
        write_settings(&path, &settings).unwrap();
        settings.parallel_downloads = 7;
        write_settings(&path, &settings).unwrap();
        assert_eq!(read_settings(&path).parallel_downloads, 7);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
    #[test]
    fn concurrent_readers_always_see_complete_settings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        write_settings(&path, &Settings::default()).unwrap();
        let writer_path = path.clone();
        let writer = std::thread::spawn(move || {
            for count in 1..20 {
                let mut settings = Settings::default();
                settings.parallel_downloads = count;
                write_settings(&writer_path, &settings).unwrap();
            }
        });
        for _ in 0..200 {
            let body = fs::read_to_string(&path).unwrap();
            let settings: Settings = serde_json::from_str(&body).unwrap();
            assert!((1..20).contains(&settings.parallel_downloads));
        }
        writer.join().unwrap();
    }

}
