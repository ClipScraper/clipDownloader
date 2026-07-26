use tauri::command;

use crate::database::{Database, DeleteMode, OutputFormat, UiBacklogRow};

/// Run blocking rusqlite/fs work off the async runtime so it can't stall other
/// commands or the download manager.
async fn run_blocking<T: Send + 'static, F: FnOnce() -> Result<T, String> + Send + 'static>(f: F) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| format!("Join error: {e}"))?
}

/// Return all backlog rows, already normalized for the UI.
#[command]
pub async fn list_backlog() -> Result<Vec<UiBacklogRow>, String> {
    run_blocking(|| {
        let db = Database::new().map_err(|e| e.to_string())?;
        db.list_backlog_ui().map_err(|e| e.to_string())
    })
    .await
}

/// Return all queue rows, normalized for the UI.
#[command]
pub async fn list_queue() -> Result<Vec<UiBacklogRow>, String> {
    run_blocking(|| {
        let db = Database::new().map_err(|e| e.to_string())?;
        db.list_queue_ui().map_err(|e| e.to_string())
    })
    .await
}

/* ---- mutations: move → queue ---- */

#[command]
pub async fn move_link_to_queue(link: String) -> Result<u64, String> {
    run_blocking(move || {
        let db = Database::new().map_err(|e| e.to_string())?;
        db.move_link_to_queue(&link).map(|n| n as u64).map_err(|e| e.to_string())
    })
    .await
}

#[command]
pub async fn move_collection_to_queue(platform: String, handle: String, content_type: String) -> Result<u64, String> {
    run_blocking(move || {
        let db = Database::new().map_err(|e| e.to_string())?;
        // DB stores origin in lowercase already; UI provides lowercase tokens too.
        db.move_collection_to_queue(&platform, &handle, &content_type).map(|n| n as u64).map_err(|e| e.to_string())
    })
    .await
}

#[command]
pub async fn move_platform_to_queue(platform: String) -> Result<u64, String> {
    run_blocking(move || {
        let db = Database::new().map_err(|e| e.to_string())?;
        db.move_platform_to_queue(&platform).map(|n| n as u64).map_err(|e| e.to_string())
    })
    .await
}

/* ---- mutations: move → backlog ---- */

#[command]
pub async fn move_link_to_backlog(link: String) -> Result<u64, String> {
    run_blocking(move || {
        let db = Database::new().map_err(|e| e.to_string())?;
        db.move_link_to_backlog(&link).map(|n| n as u64).map_err(|e| e.to_string())
    })
    .await
}

#[command]
pub async fn move_collection_to_backlog(platform: String, handle: String, content_type: String) -> Result<u64, String> {
    run_blocking(move || {
        let db = Database::new().map_err(|e| e.to_string())?;
        db.move_collection_to_backlog(&platform, &handle, &content_type).map(|n| n as u64).map_err(|e| e.to_string())
    })
    .await
}

#[command]
pub async fn move_platform_to_backlog(platform: String) -> Result<u64, String> {
    run_blocking(move || {
        let db = Database::new().map_err(|e| e.to_string())?;
        db.move_platform_to_backlog(&platform).map(|n| n as u64).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn toggle_output_format(link: String) -> Result<(), String> {
    run_blocking(move || {
        let db = Database::new().map_err(|e| e.to_string())?;
        db.toggle_output_format_for_link(&link).map(|_| ()).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn set_output_format(link: String, format: String) -> Result<(), String> {
    run_blocking(move || {
        let fmt = match format.to_lowercase().as_str() {
            "audio" => OutputFormat::Audio,
            "video" => OutputFormat::Video,
            _ => OutputFormat::Default,
        };
        let db = Database::new().map_err(|e| e.to_string())?;
        db.set_output_format_for_link(&link, fmt).map(|_| ()).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn list_done() -> Result<Vec<UiBacklogRow>, String> {
    run_blocking(|| {
        let db = Database::new().map_err(|e| e.to_string())?;
        db.list_done_ui().map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn list_downloads() -> Result<Vec<UiBacklogRow>, String> {
    run_blocking(|| {
        let db = Database::new().map_err(|e| e.to_string())?;
        db.list_all_ui().map_err(|e| e.to_string())
    })
    .await
}

/* ---- deletions: honor delete_mode ---- */

#[tauri::command]
pub async fn delete_rows_by_platform(platform: String) -> Result<u64, String> {
    run_blocking(move || {
        use std::fs;
        let db = Database::new().map_err(|e| e.to_string())?;
        let mode = crate::settings::load_settings().delete_mode;
        if matches!(mode, DeleteMode::Hard) {
            let pairs = db.list_ids_and_paths_by_platform(&platform).map_err(|e| e.to_string())?;
            for (_id, path) in pairs {
                if !path.is_empty() && path != "unknown_path" {
                    let _ = fs::remove_file(path);
                }
            }
        }
        db.delete_by_platform(&platform).map(|n| n as u64).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn delete_rows_by_collection(platform: String, handle: String, origin: String) -> Result<u64, String> {
    run_blocking(move || {
        use std::fs;
        let db = Database::new().map_err(|e| e.to_string())?;
        let mode = crate::settings::load_settings().delete_mode;
        if matches!(mode, DeleteMode::Hard) {
            let pairs = db.list_ids_and_paths_by_collection(&platform, &handle, &origin).map_err(|e| e.to_string())?;
            for (_id, path) in pairs {
                if !path.is_empty() && path != "unknown_path" {
                    let _ = fs::remove_file(path);
                }
            }
        }
        db.delete_by_collection(&platform, &handle, &origin).map(|n| n as u64).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn delete_rows_by_link(link: String) -> Result<u64, String> {
    run_blocking(move || {
        use std::fs;
        let db = Database::new().map_err(|e| e.to_string())?;
        let mode = crate::settings::load_settings().delete_mode;
        if matches!(mode, DeleteMode::Hard) {
            let pairs = db.list_ids_and_paths_by_link(&link).map_err(|e| e.to_string())?;
            for (_id, path) in pairs {
                if !path.is_empty() && path != "unknown_path" {
                    let _ = fs::remove_file(&path);
                }
            }
        }
        db.delete_by_link(&link).map(|n| n as u64).map_err(|e| e.to_string())
    })
    .await
}
