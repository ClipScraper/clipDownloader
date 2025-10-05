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
