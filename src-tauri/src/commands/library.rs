use std::path::PathBuf;
use tauri_plugin_opener::OpenerExt;

fn open_with_default_app(app: &tauri::AppHandle, path: &str) -> Result<(), String> {
    app.opener().open_path(path, None::<&str>).map_err(|e| format!("failed to open path: {e}"))
}

fn open_folder(app: &tauri::AppHandle, path: &str) -> Result<(), String> {
    open_with_default_app(app, path)
}

fn path_exists_ok(path: &str) -> bool {
    let p = PathBuf::from(path);
    p.exists() && p.is_file()
}

#[tauri::command]
pub async fn open_file_for_link(app: tauri::AppHandle, link: String) -> Result<(), String> {
    let db = crate::database::Database::new().map_err(|e| e.to_string())?;
    let Some((_id, path)) = db.find_done_row_by_link(&link).map_err(|e| e.to_string())? else {
        return Ok(());
    };
    if !path_exists_ok(&path) {
        return Ok(());
    }
    open_with_default_app(&app, &path)
}

#[tauri::command]
pub async fn open_folder_for_link(app: tauri::AppHandle, link: String) -> Result<(), String> {
    let db = crate::database::Database::new().map_err(|e| e.to_string())?;
    let Some((_id, path)) = db.find_done_row_by_link(&link).map_err(|e| e.to_string())? else {
        return Ok(());
    };
    let p = PathBuf::from(&path);
    let Some(dir) = p.parent().filter(|dir| dir.is_dir()) else {
        return Ok(());
    };
    open_folder(&app, &dir.to_string_lossy())
}

#[tauri::command]
pub async fn open_platform_folder(app: tauri::AppHandle, platform: String) -> Result<(), String> {
    let s = crate::settings::load_settings();
    let base = std::path::PathBuf::from(s.download_directory);
    let p = base.join(platform);
    if !p.exists() {
        return Ok(());
    }
    open_folder(&app, &p.to_string_lossy())
}

#[tauri::command]
pub async fn open_collection_folder(app: tauri::AppHandle, platform: String, handle: String, content_type: String) -> Result<(), String> {
    let s = crate::settings::load_settings();
    let base = std::path::PathBuf::from(s.download_directory);
    let label = crate::database::Database::collection_folder_label(&content_type, &handle);
    let p = base.join(platform).join(label);
    if !p.exists() {
        return Ok(());
    }
    open_folder(&app, &p.to_string_lossy())
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
