use crate::database::{DefaultOutput, DeleteMode, OnDuplicate, Settings};
use once_cell::sync::Lazy;
use std::sync::RwLock;
use std::{
    fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;

/// In-memory settings cache. `load_settings` is called many times per download
/// (pipeline + each yt-dlp/gallery-dl helper); without a cache each call did a
/// file read, a temp-file write-probe, and a full JSON rewrite. The cache is
/// populated on first read and refreshed by `save_settings`.
static SETTINGS_CACHE: Lazy<RwLock<Option<Settings>>> = Lazy::new(|| RwLock::new(None));

fn cache_get() -> Option<Settings> {
    SETTINGS_CACHE.read().ok().and_then(|g| g.clone())
}

fn cache_set(settings: &Settings) {
    if let Ok(mut g) = SETTINGS_CACHE.write() {
        *g = Some(settings.clone());
    }
}

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
        Settings {id: None, download_directory: default_download_dir().to_string_lossy().to_string(), on_duplicate: OnDuplicate::CreateNew, delete_mode: DeleteMode::Soft, debug_logs: false, default_output: DefaultOutput::Video, download_automatically: true, keep_downloading_on_other_pages: true, parallel_downloads: 3, use_system_binaries: false, cooldown_secs: 0, retry_on_queue_empty: false}
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

/// Load settings, using the in-memory cache when warm. The first (cold) call
/// reads from disk, validates the download directory, and persists any fixups;
/// subsequent calls are served from memory until `save_settings` refreshes it.
pub fn load_settings() -> Settings {
    if let Some(cached) = cache_get() {
        return cached;
    }
    let settings = load_settings_from_disk();
    cache_set(&settings);
    settings
}

/// Load settings from JSON, **validate the download path**, and persist any fixups.
fn load_settings_from_disk() -> Settings {
    let path = settings_json_path();

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    // Read or default
    let mut settings = match fs::read_to_string(&path) {
        Ok(s) => serde_json::from_str::<Settings>(&s).unwrap_or_default(),
        Err(_) => Settings::default(),
    };

    // Validate / normalize download directory
    let fixed_dir = validated_download_dir(&settings.download_directory);
    if settings.download_directory != fixed_dir {
        settings.download_directory = fixed_dir;
    }

    // Persist the clean copy (also migrates any old/invalid file)
    let _ = fs::write(&path, serde_json::to_string_pretty(&settings).unwrap_or_else(|_| "{}".into()));

    settings
}

/// Save settings back to JSON (and ensure the target directory exists).
/// Also validates the directory; if invalid, we fall back to the default Downloads path.
pub fn save_settings(settings: &Settings) -> Result<(), String> {
    let path = settings_json_path();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create settings dir: {e}"))?;
    }

    let final_dir = validated_download_dir(&settings.download_directory);
    let to_write = Settings {id: settings.id, download_directory: final_dir, on_duplicate: settings.on_duplicate.clone(), delete_mode: settings.delete_mode.clone(), debug_logs: settings.debug_logs, default_output: settings.default_output.clone(), download_automatically: settings.download_automatically, keep_downloading_on_other_pages: settings.keep_downloading_on_other_pages, parallel_downloads: settings.parallel_downloads, use_system_binaries: settings.use_system_binaries, cooldown_secs: settings.cooldown_secs, retry_on_queue_empty: settings.retry_on_queue_empty};

    let body = serde_json::to_string_pretty(&to_write).map_err(|e| format!("Failed to serialize settings: {e}"))?;
    fs::write(&path, body).map_err(|e| format!("Failed to write settings.json: {e}"))?;

    // Keep the cache in sync so readers see the update without touching disk.
    cache_set(&to_write);
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
