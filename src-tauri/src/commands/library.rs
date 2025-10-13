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

fn open_folder(path: &str) -> Result<(), String> {
    let p = PathBuf::from(path);
    let dir = p.parent().ok_or_else(|| "no parent folder".to_string())?;
    let dir_str = dir.to_string_lossy().to_string();

    #[cfg(target_os = "windows")]
    {
        Command::new("explorer")
            .arg(dir_str)
            .spawn()
            .map_err(|e| format!("failed to open folder: {e}"))?;
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(dir_str)
            .spawn()
            .map_err(|e| format!("failed to open folder: {e}"))?;
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
            .arg(dir_str)
            .spawn()
            .map_err(|e| format!("failed to open folder: {e}"))?;
    }
    Ok(())
}

fn path_exists_ok(path: &str) -> bool {
    let p = PathBuf::from(path);
    p.exists() && p.is_file()
}

#[tauri::command]
pub async fn open_file_for_link(link: String) -> Result<(), String> {
    let db = crate::database::Database::new().map_err(|e| e.to_string())?;
    let some = db
        .find_done_row_by_link(&link)
        .map_err(|e| e.to_string())?;

    let (_id, path) = some.ok_or_else(|| "no library item found for link".to_string())?;
    if !path_exists_ok(&path) {
        return Err(format!("file not found: {path}"));
    }
    open_with_default_app(&path)
}

#[tauri::command]
pub async fn open_folder_for_link(link: String) -> Result<(), String> {
    let db = crate::database::Database::new().map_err(|e| e.to_string())?;
    let some = db
        .find_done_row_by_link(&link)
        .map_err(|e| e.to_string())?;

    let (_id, path) = some.ok_or_else(|| "no library item found for link".to_string())?;
    if !PathBuf::from(&path).exists() {
        return Err(format!("path not found: {path}"));
    }
    open_folder(&path)
}

#[tauri::command]
pub async fn open_platform_folder(platform: String) -> Result<(), String> {
    let s = crate::settings::load_settings();
    let base = std::path::PathBuf::from(s.download_directory);
    let p = base.join(platform);
    if !p.exists() {
        return Err(format!("path not found: {}", p.display()));
    }
    open_folder(&p.to_string_lossy())
}

#[tauri::command]
pub async fn open_collection_folder(platform: String, handle: String, content_type: String) -> Result<(), String> {
    let s = crate::settings::load_settings();
    let base = std::path::PathBuf::from(s.download_directory);
    let label = crate::database::Database::collection_folder_label(&content_type, &handle);
    let p = base.join(platform).join(label);
    if !p.exists() {
        return Err(format!("path not found: {}", p.display()));
    }
    open_folder(&p.to_string_lossy())
}

#[tauri::command]
pub async fn delete_library_item(link: String) -> Result<(), String> {
    use std::fs;

    let db = crate::database::Database::new().map_err(|e| e.to_string())?;
    let some = db
        .find_done_row_by_link(&link)
        .map_err(|e| e.to_string())?;

    let (id, path) = some.ok_or_else(|| "no library item found for link".to_string())?;

    // Best-effort file delete first
    if !path.is_empty() && path != "unknown_path" {
        if let Err(e) = fs::remove_file(&path) {
            // If it doesn't exist, that's fine; otherwise surface error
            if std::io::ErrorKind::NotFound != e.kind() {
                return Err(format!("failed to delete file: {e}"));
            }
        }
    }

    // Remove DB row
    let _ = db.delete_row_by_id(id).map_err(|e| e.to_string())?;
    Ok(())
}
