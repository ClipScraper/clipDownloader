use std::path::PathBuf;
use std::process::Command;

fn open_with_default_app(path: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        // Use PowerShell to avoid `start` quoting quirks
        Command::new("powershell")
            .args(&["-NoProfile", "-Command", "Start-Process", path])
            .spawn()
            .map_err(|e| format!("failed to open file: {e}"))?;
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|e| format!("failed to open file: {e}"))?;
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| format!("failed to open file: {e}"))?;
    }
    Ok(())
}

/// Open a directory itself in the OS file manager.
fn reveal_dir(dir: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        Command::new("explorer")
            .arg(dir)
            .spawn()
            .map_err(|e| format!("failed to open folder: {e}"))?;
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(dir)
            .spawn()
            .map_err(|e| format!("failed to open folder: {e}"))?;
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
            .arg(dir)
            .spawn()
            .map_err(|e| format!("failed to open folder: {e}"))?;
    }
    Ok(())
}

/// Resolve a path stored in the database to one that actually exists on disk.
///
/// Older builds mis-decoded yt-dlp's cp1252 stdout as UTF-8, corrupting any
/// non-ASCII characters in the stored download path (e.g. `Téléchargements`
/// became `T�l�chargements`). Only the download-root prefix is affected, so if
/// the stored path is missing we re-root the below-root components under the
/// correctly-encoded `download_directory` from settings.
fn resolve_stored_path(path: &str) -> Option<PathBuf> {
    let p = PathBuf::from(path);
    if p.exists() {
        return Some(p);
    }
    let root = PathBuf::from(crate::settings::load_settings().download_directory);
    let root_depth = root.components().count();
    let comps: Vec<_> = p.components().collect();
    if comps.len() > root_depth {
        let mut rebuilt = root;
        for c in &comps[root_depth..] {
            rebuilt.push(c.as_os_str());
        }
        if rebuilt.exists() {
            return Some(rebuilt);
        }
    }
    None
}

#[tauri::command]
pub async fn open_file_for_link(link: String) -> Result<(), String> {
    let db = crate::database::Database::new().map_err(|e| e.to_string())?;
    let Some((_id, path)) = db.find_done_row_by_link(&link).map_err(|e| e.to_string())? else {
        return Ok(());
    };
    let Some(real) = resolve_stored_path(&path).filter(|p| p.is_file()) else {
        return Ok(());
    };
    open_with_default_app(&real.to_string_lossy())
}

#[tauri::command]
pub async fn open_folder_for_link(link: String) -> Result<(), String> {
    let s = crate::settings::load_settings();
    let db = crate::database::Database::new().map_err(|e| e.to_string())?;
    let Some((_id, path)) = db.find_done_row_by_link(&link).map_err(|e| e.to_string())? else {
        return Ok(());
    };

    // Prefer the folder containing the resolved file; otherwise fall back to
    // the configured download root so the user still lands somewhere useful.
    let dir = resolve_stored_path(&path)
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .filter(|d| d.exists())
        .or_else(|| {
            let root = PathBuf::from(&s.download_directory);
            root.exists().then_some(root)
        });

    match dir {
        Some(d) => reveal_dir(&d.to_string_lossy()),
        None => Ok(()),
    }
}

#[tauri::command]
pub async fn open_platform_folder(platform: String) -> Result<(), String> {
    let s = crate::settings::load_settings();
    let base = std::path::PathBuf::from(s.download_directory);
    let p = base.join(platform);
    if !p.exists() {
        return Ok(());
    }
    reveal_dir(&p.to_string_lossy())
}

#[tauri::command]
pub async fn open_collection_folder(
    platform: String,
    handle: String,
    content_type: String,
) -> Result<(), String> {
    let s = crate::settings::load_settings();
    let base = std::path::PathBuf::from(s.download_directory);
    let label = crate::database::Database::collection_folder_label(&content_type, &handle);
    let p = base.join(platform).join(label);
    if !p.exists() {
        return Ok(());
    }
    reveal_dir(&p.to_string_lossy())
}

#[tauri::command]
pub async fn delete_library_item(link: String) -> Result<(), String> {
    use std::fs;

    let db = crate::database::Database::new().map_err(|e| e.to_string())?;
    let some = db.find_done_row_by_link(&link).map_err(|e| e.to_string())?;

    let (id, path) = some.ok_or_else(|| "no library item found for link".to_string())?;

    // Respect delete mode
    let mode = crate::settings::load_settings().delete_mode;
    if matches!(mode, crate::database::DeleteMode::Hard) {
        // Best-effort file delete first
        if !path.is_empty() && path != "unknown_path" {
            if let Err(e) = fs::remove_file(&path) {
                // If it doesn't exist, that's fine; otherwise surface error
                if std::io::ErrorKind::NotFound != e.kind() {
                    return Err(format!("failed to delete file: {e}"));
                }
            }
        }
    }

    // Remove DB row
    let _ = db.delete_row_by_id(id).map_err(|e| e.to_string())?;
    Ok(())
}
