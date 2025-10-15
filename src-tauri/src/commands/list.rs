use tauri::command;

/// Return all backlog rows, already normalized for the UI.
#[command]
pub async fn list_backlog() -> Result<Vec<crate::database::UiBacklogRow>, String> {
    let db = crate::database::Database::new().map_err(|e| e.to_string())?;
    db.list_backlog_ui().map_err(|e| e.to_string())
}

/// Return all queue rows, normalized for the UI.
#[command]
pub async fn list_queue() -> Result<Vec<crate::database::UiBacklogRow>, String> {
    let db = crate::database::Database::new().map_err(|e| e.to_string())?;
    db.list_queue_ui().map_err(|e| e.to_string())
}

/* ---- mutations: move → queue ---- */

#[command]
pub async fn move_link_to_queue(link: String) -> Result<u64, String> {
    let db = crate::database::Database::new().map_err(|e| e.to_string())?;
    let n = db.move_link_to_queue(&link).map_err(|e| e.to_string())?;
    Ok(n as u64)
}

#[command]
pub async fn move_collection_to_queue(platform: String, handle: String, content_type: String) -> Result<u64, String> {
    let db = crate::database::Database::new().map_err(|e| e.to_string())?;
    // DB stores origin in lowercase already; UI provides lowercase tokens too.
    let n = db
        .move_collection_to_queue(&platform, &handle, &content_type)
        .map_err(|e| e.to_string())?;
    Ok(n as u64)
}

#[command]
pub async fn move_platform_to_queue(platform: String) -> Result<u64, String> {
    let db = crate::database::Database::new().map_err(|e| e.to_string())?;
    let n = db
        .move_platform_to_queue(&platform)
        .map_err(|e| e.to_string())?;
    Ok(n as u64)
}

#[tauri::command]
pub async fn list_done() -> Result<Vec<crate::database::UiBacklogRow>, String> {
    let db = crate::database::Database::new().map_err(|e| e.to_string())?;
    db.list_done_ui().map_err(|e| e.to_string())
}

/* ---- deletions: honor delete_mode ---- */

#[tauri::command]
pub async fn delete_rows_by_platform(platform: String) -> Result<u64, String> {
    use std::fs;
    let db = crate::database::Database::new().map_err(|e| e.to_string())?;
    let mode = crate::settings::load_settings().delete_mode;
    let pairs = db
        .list_ids_and_paths_by_platform(&platform)
        .map_err(|e| e.to_string())?;
    let (ids, paths): (Vec<_>, Vec<_>) = pairs.into_iter().unzip();
    if matches!(mode, crate::database::DeleteMode::Hard) {
        for p in paths.into_iter() {
            if !p.is_empty() && p != "unknown_path" { let _ = fs::remove_file(p); }
        }
    }
    let mut deleted: u64 = 0;
    for id in ids.into_iter() {
        deleted += db.delete_row_by_id(id).map_err(|e| e.to_string())? as u64;
    }
    Ok(deleted)
}

#[tauri::command]
pub async fn delete_rows_by_collection(platform: String, handle: String, origin: String) -> Result<u64, String> {
    use std::fs;
    let db = crate::database::Database::new().map_err(|e| e.to_string())?;
    let mode = crate::settings::load_settings().delete_mode;
    let pairs = db
        .list_ids_and_paths_by_collection(&platform, &handle, &origin)
        .map_err(|e| e.to_string())?;
    let (ids, paths): (Vec<_>, Vec<_>) = pairs.into_iter().unzip();
    if matches!(mode, crate::database::DeleteMode::Hard) {
        for p in paths.into_iter() {
            if !p.is_empty() && p != "unknown_path" { let _ = fs::remove_file(p); }
        }
    }
    let mut deleted: u64 = 0;
    for id in ids.into_iter() {
        deleted += db.delete_row_by_id(id).map_err(|e| e.to_string())? as u64;
    }
    Ok(deleted)
}

#[tauri::command]
pub async fn delete_rows_by_link(link: String) -> Result<u64, String> {
    use std::fs;
    let db = crate::database::Database::new().map_err(|e| e.to_string())?;
    let mode = crate::settings::load_settings().delete_mode;
    let pairs = db
        .list_ids_and_paths_by_link(&link)
        .map_err(|e| e.to_string())?;
    let mut deleted: u64 = 0;
    for (id, path) in pairs.into_iter() {
        if matches!(mode, crate::database::DeleteMode::Hard) {
            if !path.is_empty() && path != "unknown_path" { let _ = fs::remove_file(&path); }
        }
        deleted += db.delete_row_by_id(id).map_err(|e| e.to_string())? as u64;
    }
    Ok(deleted)
}
