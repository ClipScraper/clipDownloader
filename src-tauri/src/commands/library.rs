use crate::utils::resolve_stored_path;
use std::path::PathBuf;
use std::process::Command;

fn open_with_default_app(path: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        // Use PowerShell to avoid `start` quoting quirks
        Command::new("powershell").args(&["-NoProfile", "-Command", "Start-Process", path]).spawn().map_err(|e| format!("failed to open file: {e}"))?;
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(path).spawn().map_err(|e| format!("failed to open file: {e}"))?;
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open").arg(path).spawn().map_err(|e| format!("failed to open file: {e}"))?;
    }
    Ok(())
}

/// Open a directory itself in the OS file manager.
fn reveal_dir(dir: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        Command::new("explorer").arg(dir).spawn().map_err(|e| format!("failed to open folder: {e}"))?;
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(dir).spawn().map_err(|e| format!("failed to open folder: {e}"))?;
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open").arg(dir).spawn().map_err(|e| format!("failed to open folder: {e}"))?;
    }
    Ok(())
}

#[tauri::command]
pub async fn open_file_for_link(app: tauri::AppHandle, link: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let db = crate::database::Database::new().map_err(|e| e.to_string())?;
        let Some((_id, path)) = db.find_done_row_by_link(&link).map_err(|e| e.to_string())? else {
            return Ok(());
        };
        let Some(real) = resolve_stored_path(&path).filter(|p| p.is_file()) else {
            return Ok(());
        };
        if real.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("url")) {
            use tauri_plugin_opener::OpenerExt;
            let url = crate::utils::url::http_url(&link)?;
            return app.opener().open_url(url.as_str(), None::<&str>).map_err(|e| e.to_string());
        }
        open_with_default_app(&real.to_string_lossy())
    })
    .await
    .map_err(|e| format!("Join error: {e}"))?
}

#[tauri::command]
pub async fn open_folder_for_link(link: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let s = crate::settings::load_settings();
        let db = crate::database::Database::new().map_err(|e| e.to_string())?;
        let Some((_id, path)) = db.find_done_row_by_link(&link).map_err(|e| e.to_string())? else {
            return Ok(());
        };

        // Prefer the folder containing the resolved file; otherwise fall back to
        // the configured download root so the user still lands somewhere useful.
        let dir = resolve_stored_path(&path).and_then(|p| p.parent().map(|d| d.to_path_buf())).filter(|d| d.exists()).or_else(|| {
            let root = PathBuf::from(&s.download_directory);
            root.exists().then_some(root)
        });

        match dir {
            Some(d) => reveal_dir(&d.to_string_lossy()),
            None => Ok(()),
        }
    })
    .await
    .map_err(|e| format!("Join error: {e}"))?
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
pub async fn open_collection_folder(platform: String, handle: String, content_type: String) -> Result<(), String> {
    let s = crate::settings::load_settings();
    let base = std::path::PathBuf::from(s.download_directory);
    let previous = crate::database::Database::collection_directory(&base, &platform, &content_type, &handle, "");
    if platform == "patreon" && content_type == "collection" {
        return tauri::async_runtime::spawn_blocking(move || {
            let db = crate::database::Database::new().map_err(|e| e.to_string())?;
            let creator = db.patreon_collection_creator(&handle).map_err(|e| e.to_string())?;
            let folder = crate::database::Database::collection_directory(&base, &platform, &content_type, &handle, &creator);
            for dir in [folder, previous] {
                if dir.is_dir() { return reveal_dir(&dir.to_string_lossy()); }
            }
            // Earlier imports may have saved files under an ID-only collection label.
            let paths = db.list_ids_and_paths_by_collection(&platform, &handle, &content_type).map_err(|e| e.to_string())?;
            for (_, path) in paths {
                if path.is_empty() || path == "unknown_path" {
                    continue;
                }
                if let Some(file) = resolve_stored_path(&path).filter(|p| p.is_file()) {
                    if let Some(dir) = file.parent() { return reveal_dir(&dir.to_string_lossy()); }
                }
            }
            Ok(())
        }).await.map_err(|e| e.to_string())?;
    }
    if previous.is_dir() { return reveal_dir(&previous.to_string_lossy()); }
    Ok(())
}

#[tauri::command]
pub async fn delete_library_item(link: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
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
    })
    .await
    .map_err(|e| format!("Join error: {e}"))?
}

#[tauri::command]
pub async fn list_collected_links() -> Result<Vec<crate::collected_links::CollectedLink>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let db = crate::database::Database::new().map_err(|e| e.to_string())?;
        db.collected_links().map_err(|e| e.to_string())
    }).await.map_err(|e| format!("Join error: {e}"))?
}

#[tauri::command]
pub async fn delete_collected_links(app: tauri::AppHandle, selection: crate::collected_links::LinkSelection) -> Result<(), String> {
    use tauri::Emitter;
    tauri::async_runtime::spawn_blocking(move || {
        let db = crate::database::Database::new().map_err(|e| e.to_string())?;
        db.delete_collected_links(&selection).map_err(|e| e.to_string())
    }).await.map_err(|e| format!("Join error: {e}"))??;
    let _ = app.emit("post-links-collected", ());
    Ok(())
}

#[tauri::command]
pub fn open_collected_link(app: tauri::AppHandle, url: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let url = crate::utils::url::http_url(&url)?;
    app.opener().open_url(url.as_str(), None::<&str>).map_err(|e| e.to_string())
}
